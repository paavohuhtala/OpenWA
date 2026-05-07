//! `FireEffect` — animated fire/plasma overlay rendered on per-team
//! turn-order health bars as a ranked-MP rank reward.
//!
//! Two reward tiers exist; the per-team
//! [`fire_effect_tier`](crate::engine::game_info::GameInfoTeamRecord::fire_effect_tier)
//! gate selects between [`GameWorld::fire_palette_high`](crate::engine::world::GameWorld)
//! (`tier >= 2`) and [`GameWorld::fire_palette_low`](crate::engine::world::GameWorld)
//! (`tier == 1`). With both populated by the network/ranked handshake,
//! `TurnOrderTeamEntry__Render` (0x00563620) drives the effect each frame.

use core::mem;

use crate::FieldRegistry;
use crate::bitgrid::{BIT_GRID_DISPLAY_VTABLE, BitGrid, BitGridDisplayVtable, DisplayBitGrid};
use crate::rebase::rb;
use crate::wa_alloc::{wa_free, wa_malloc_struct_zeroed, wa_malloc_zeroed};
use openwa_core::rng::wa_lcg;

crate::define_addresses! {
    class "FireEffect" {
        /// `__usercall(EAX=dt, ESI=this)`.
        fn/Usercall FIRE_EFFECT_STEP_FIRE = 0x00524240;
    }
}

#[openwa_game::vtable(size = 4, va = 0x00664698, class = "FireEffect")]
pub struct FireEffectVtable {
    pub destructor: fn(this: *mut FireEffect, flags: u8) -> *mut FireEffect,
    pub tick: fn(this: *mut FireEffect, dt: i32),
    pub apply_palette: fn(this: *mut FireEffect, src: *mut DisplayBitGrid) -> *mut DisplayBitGrid,
    /// 0x00524480 — unidentified pixel transform; no callers found.
    pub sub_524480: fn(this: *mut FireEffect, arg: *mut DisplayBitGrid) -> *mut DisplayBitGrid,
}

#[derive(FieldRegistry)]
#[repr(C)]
pub struct FireEffect {
    /// 0x000
    pub vtable: *const FireEffectVtable,
    /// 0x004: Always 0x68 (104).
    pub width: u32,
    /// 0x008: Always 0x1C (28).
    pub height: u32,
    /// 0x00C: Constructor seeds it with `0x12345678`.
    pub rng_state: u32,
    /// 0x010
    pub layer_front: *mut DisplayBitGrid,
    /// 0x014
    pub layer_back: *mut DisplayBitGrid,
    /// 0x018: Palette-remapped output blitted onto the team bar.
    pub layer_output: *mut DisplayBitGrid,
    /// 0x01C: Cooling LUT — input is the sum of 9 neighbour pixels
    /// (range 0..=2295 = 9*255), output is the cooled value (0..=254).
    /// Generated as `entry[i] = max(0, (i / 9) - (1 if i % 9 < 6 else 0))`.
    pub cooling_lut: [u8; 0x8F8],
    /// 0x914
    pub tick_accum: u32,
    /// 0x918: Advanced by `dt * 20` per [`Self::step_fire`] call; one spark
    /// generated per 100 units.
    pub spark_accum: u32,
    /// 0x91C: Set by [`Self::apply_palette`]; [`Self::tick`] is a no-op
    /// while this is 0.
    pub active: u32,
    pub _pad_920: [u8; 0x20],
}

const _: () = assert!(core::mem::size_of::<FireEffect>() == 0x940);

const FIRE_EFFECT_LAYER_WIDTH: u32 = 104;
const FIRE_EFFECT_LAYER_HEIGHT: u32 = 28;

impl FireEffect {
    pub unsafe fn alloc() -> *mut FireEffect {
        unsafe {
            let effect = wa_malloc_struct_zeroed::<FireEffect>();
            if effect.is_null() {
                return core::ptr::null_mut();
            }

            (*effect).vtable = rb(FIRE_EFFECT_VTABLE) as *const FireEffectVtable;
            (*effect).width = FIRE_EFFECT_LAYER_WIDTH;
            (*effect).height = FIRE_EFFECT_LAYER_HEIGHT;

            (*effect).layer_front = alloc_display_layer();
            (*effect).layer_back = alloc_display_layer();
            (*effect).layer_output = alloc_display_layer();

            for i in 1..0x8F8u32 {
                let q = (i / 9) as i32;
                let mut v = if (i % 9) < 6 { q - 1 } else { q };
                if v < 0 {
                    v = 0;
                }
                (*effect).cooling_lut[i as usize] = v as u8;
            }

            (*effect).rng_state = 0x12345678;
            effect
        }
    }

    pub unsafe extern "thiscall" fn tick(this: *mut FireEffect, dt: i32) {
        unsafe {
            if (*this).active == 0 {
                return;
            }
            (*this).tick_accum = (*this).tick_accum.wrapping_add(dt as u32);
            while (*this).tick_accum as i32 >= 100 {
                Self::step_fire(this, dt);
                (*this).tick_accum = (*this).tick_accum.wrapping_sub(100);
            }
        }
    }

    pub unsafe fn step_fire(this: *mut FireEffect, dt: i32) {
        unsafe {
            let height = (*this).height as i32;
            let width = (*this).width as i32;

            (*this).spark_accum = (*this).spark_accum.wrapping_add((dt * 20) as u32);
            while (*this).spark_accum as i32 >= 100 {
                let r1 = wa_lcg((*this).rng_state);
                let r2 = wa_lcg(r1);
                (*this).rng_state = r2;

                let y = height - ((r2 >> 16) & 7) as i32 - 2;
                let span = (width - 2) as u32;
                let x = ((r1 >> 16) % span) as i32 + 1;

                let layer = (*this).layer_front;
                let vt = (*layer).vtable;
                ((*vt).put_pixel_clipped)(layer, x, y, 0xff);

                (*this).spark_accum = (*this).spark_accum.wrapping_sub(100);
            }

            // At y = height - 1 the read of row y + 1 is one row past the
            // official end of the front buffer. WA's original does the same
            // and survives because its heap allocator returns oversized
            // chunks. Using raw pointer offsets here so a bounds-checked
            // slice doesn't panic on that overshoot.
            let front_data = (*(*this).layer_front).data;
            let back_data = (*(*this).layer_back).data;
            let stride = (*(*this).layer_back).row_stride as isize;
            let cooling_lut_base = (*this).cooling_lut.as_ptr();

            for y in 2..(height as isize) {
                let row_centre = front_data.offset(y * stride);
                let row_above = front_data.offset((y - 1) * stride);
                let row_below = front_data.offset((y + 1) * stride);
                for x in 1..(width as isize - 1) {
                    let sum = *row_above.offset(x - 1) as u32
                        + *row_above.offset(x) as u32
                        + *row_above.offset(x + 1) as u32
                        + *row_centre.offset(x - 1) as u32
                        + *row_centre.offset(x) as u32
                        + *row_centre.offset(x + 1) as u32
                        + *row_below.offset(x - 1) as u32
                        + *row_below.offset(x) as u32
                        + *row_below.offset(x + 1) as u32;

                    let cooled = *cooling_lut_base.add(sum as usize);
                    *back_data.offset((y - 1) * stride + x) = cooled;
                }
            }

            mem::swap(&mut (*this).layer_front, &mut (*this).layer_back);
        }
    }

    /// `src`'s pixel buffer is a 256-entry intensity-to-palette-index LUT.
    pub unsafe extern "thiscall" fn apply_palette(
        this: *mut FireEffect,
        src: *mut DisplayBitGrid,
    ) -> *mut DisplayBitGrid {
        unsafe {
            if src.is_null() {
                return (*this).layer_front;
            }

            (*this).active = 1;

            let lut_ptr = (*src).data as *const u8;
            let front = (*this).layer_front;
            let output = (*this).layer_output;
            let width = (*this).width as i32;
            let height = (*this).height as i32;

            crate::bitgrid::blit::blit_impl(
                output,
                0,
                0,
                width,
                height,
                front,
                0,
                0,
                core::ptr::null(),
                0,
            );

            if (*output).cells_per_unit == 8 && !lut_ptr.is_null() {
                remap_pixels_through_lut(output, lut_ptr);
            }

            (*this).layer_output
        }
    }

    pub unsafe extern "thiscall" fn destructor(
        this: *mut FireEffect,
        flags: u8,
    ) -> *mut FireEffect {
        unsafe {
            (*this).vtable = rb(FIRE_EFFECT_VTABLE) as *const FireEffectVtable;

            for layer in [
                (*this).layer_front,
                (*this).layer_back,
                (*this).layer_output,
            ] {
                if !layer.is_null() {
                    let vt = (*layer).vtable;
                    ((*vt).destructor)(layer, 1);
                }
            }

            if flags & 1 != 0 {
                wa_free(this as *mut u8);
            }

            this
        }
    }
}

unsafe fn remap_pixels_through_lut(grid: *mut DisplayBitGrid, lut: *const u8) {
    unsafe {
        let g = &*grid;
        let mut row = g.data;
        let stride = g.row_stride as usize;
        let height = g.height as usize;
        let quads = stride / 4;

        for _ in 0..height {
            let mut p = row;
            for _ in 0..quads {
                *p = *lut.add(*p as usize);
                *p.add(1) = *lut.add(*p.add(1) as usize);
                *p.add(2) = *lut.add(*p.add(2) as usize);
                *p.add(3) = *lut.add(*p.add(3) as usize);
                p = p.add(4);
            }
            row = row.add(stride);
        }
    }
}

/// 0x4C bytes vs the 0x2C `DisplayBitGrid` proper — matches WA's original
/// `FireEffect__Constructor` allocation, which leaves 0x20 trailing bytes
/// unused.
unsafe fn alloc_display_layer() -> *mut DisplayBitGrid {
    unsafe {
        let mem = wa_malloc_zeroed(0x4C);
        if mem.is_null() {
            return core::ptr::null_mut();
        }
        BitGrid::init(
            mem as *mut BitGrid,
            8,
            FIRE_EFFECT_LAYER_WIDTH,
            FIRE_EFFECT_LAYER_HEIGHT,
        );
        let display = mem as *mut DisplayBitGrid;
        (*display).vtable = rb(BIT_GRID_DISPLAY_VTABLE) as *const BitGridDisplayVtable;
        display
    }
}
