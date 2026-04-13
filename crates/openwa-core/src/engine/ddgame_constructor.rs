//! DDGame constructor — replaces DDGame__Constructor (0x56E220)
//!
//! Despite being named DDGame__Constructor, the original function
//! receives DDGameWrapper* as `this` and creates DDGame internally.
//! It populates fields on BOTH the wrapper and the inner DDGame.
//! The Rust entry point is DDGameWrapper::create_game() in
//! ddgame_wrapper.rs; the bulk of the logic lives here because
//! it primarily initializes DDGame fields.

use super::ddgame::DDGame;
use crate::address::va;
use crate::audio::active_sound::ActiveSoundTable;
use crate::audio::dssound::DSSound;
use crate::audio::music::Music;
use crate::bitgrid::{BitGrid, BitGridBaseVtable, CollisionBitGrid, DisplayBitGrid};
use crate::engine::coord::{CoordList, CoordListEntry};
use crate::engine::ddgame_wrapper::DDGameWrapper;
use crate::engine::game_info::GameInfo;
use crate::engine::net_bridge::NetBridge;
use crate::fixed::Fixed;
use crate::input::keyboard::DDKeyboard;
use crate::rebase::rb;
use crate::render::display::gfx::DisplayGfx;
use crate::render::display::gradient::compute_complex_gradient;
use crate::render::display::palette::Palette;
use crate::render::landscape::PCLandscape;
use crate::render::sprite::gfx_dir::{
    call_gfx_find_and_load, call_gfx_load_and_wrap, call_gfx_load_dir, GfxDir, GfxDirVtable,
};
pub use crate::render::sprite::gfx_dir::{gfx_dir_find_entry, gfx_resource_create};
use crate::wa_alloc::{wa_malloc, wa_malloc_zeroed};

// ============================================================
// Pure Rust implementations of DDGame sub-functions
// ============================================================
// These are called both by create_ddgame() and by MinHook
// trampolines in openwa-dll/replacements/ddgame_init.rs.

/// Pure Rust implementation of DDGame__InitFields (0x526120).
///
/// Zeroes stride-0x194 table entries, calls init_render_indices,
/// then zeroes coordination/sound entries at 0x8Cxx and 0x98xx.
///
/// # Safety
/// `ddgame` must point to a valid, zero-filled DDGame allocation (0x98D8 bytes).
pub unsafe fn ddgame_init_fields(ddgame: *mut DDGame) {
    let base = ddgame as *mut u8;

    // Zero the stride-0x194 table (10 entries starting at 0x379C).
    // These offsets are deep in the unknown 0x2E00-0x45EB region and don't
    // have named fields yet — keep as raw offsets for now.
    for &off in &[
        0x379Cusize,
        0x3930,
        0x3AC4,
        0x3C58,
        0x3DEC,
        0x3F80,
        0x4114,
        0x42A8,
        0x443C,
        0x45D0,
    ] {
        *(base.add(off) as *mut u32) = 0;
    }

    // init_field_64d8 = TeamArena.team_count (arena+0x1EB0)
    (*ddgame).team_arena.team_count = 0;
    // init_field_72a4 = weapon_slots flat[754] = alliance 5, ammo[44]
    (*ddgame).team_arena.weapon_slots.teams[5].ammo[44] = 0;

    // InitRenderIndices — original sets ESI = ddgame + 0x72D8, now uses typed DDGame ptr
    ddgame_init_render_indices(ddgame);

    // Zero x and y of each screen coordinate entry (4 entries each)
    for entry in &mut (*ddgame).viewport_coords {
        entry.center_x = Fixed(0);
        entry.center_y = Fixed(0);
    }
    for entry in &mut (*ddgame).screen_coords_2 {
        entry.center_x = Fixed(0);
        entry.center_y = Fixed(0);
    }
}

/// Pure Rust implementation of DDGame__InitRenderIndices (0x526080).
///
/// Convention: usercall(ESI=base_ptr), plain RET.
/// Initialize render state flag, render entry table, and team index maps.
///
/// Original: called with ESI = ddgame + 0x72D8, but now uses typed DDGame fields.
///
/// # Safety
/// `ddgame` must point to a valid DDGame allocation.
pub unsafe fn ddgame_init_render_indices(ddgame: *mut DDGame) {
    (*ddgame).render_state_flag = 0;

    // Zero the active flag of each render entry (eh_vector_constructor_iterator).
    for entry in &mut (*ddgame).render_entries {
        entry.active = 0;
    }

    // Initialize all three team index maps as identity permutations.
    for map in &mut (*ddgame).team_index_maps {
        for i in 0..16i16 {
            map.entries[i as usize] = i;
        }
        map.count = 0x10;
        map.terminator = 0;
    }
}

/// Pure Rust implementation of FUN_570E20 (display layer color init).
///
/// Convention: usercall(ESI=wrapper), plain RET.
///
/// Sets display layer color parameters via display->vtable[4](layer, color).
/// Layer 1 color depends on gfx_mode and game_version.
/// Layer 2 = 0x20, Layer 3 = 0x70.
///
/// # Safety
/// `wrapper` must be a valid DDGameWrapper with initialized display and ddgame.
#[cfg(target_arch = "x86")]
pub unsafe fn display_layer_color_init(wrapper: *mut DDGameWrapper) {
    let ddgame = (*wrapper).ddgame;
    let game_info = (*ddgame).game_info;
    let game_version = (*game_info).game_version;

    // wrapper+0x4C8 is gfx_mode (at DDGameWrapper.gfx_mode)
    let layer1_color = if (*wrapper).gfx_mode == 0 {
        // (game_version > -2) - 1: yields 0 if true, -1 if false
        // Then + 0x69: yields 0x69 or 0x68
        if game_version > -2 {
            0x69i32
        } else {
            0x68i32
        }
    } else {
        5 + 0x69 // = 0x6E
    };

    let display = (*wrapper).display;
    (*display).set_layer_color(1, layer1_color);
    (*display).set_layer_color(2, 0x20);
    (*display).set_layer_color(3, 0x70);
}

/// Initialize runtime addresses for the constructor bridges.
/// Must be called once at DLL startup (from lib.rs or similar).
pub fn init_constructor_addrs() {
    unsafe {
        SPRITE_REGION_CTOR_ADDR = rb(va::SPRITE_REGION_CONSTRUCTOR);
        FUN_570A90_ADDR = rb(va::FUN_570A90);

        LOAD_SPEECH_BANKS_ADDR = rb(va::DSSOUND_LOAD_ALL_SPEECH_BANKS);
        LOADING_PROGRESS_TICK_ADDR = rb(va::DDGAME_WRAPPER_LOADING_PROGRESS_TICK);
        GFX_LOAD_SPRITES_ADDR = rb(va::GFX_DIR_LOAD_SPRITES);
    }
    crate::render::sprite::gfx_dir::init_addrs();
}

// ── Typed wrappers for WA stdcall functions ────────────────────────────────
// Each wraps a single WA function with a typed Rust signature.
// Replace with pure Rust implementations as functions are ported.

/// DDGame__InitVersionFlags (0x525BE0): sets DDGame+0x7E2E/0x7E2F/0x7E3F.
#[cfg(target_arch = "x86")]
unsafe fn wa_init_version_flags(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_VERSION_FLAGS) as usize);
    f(wrapper);
}

/// GfxHandler__LoadSprites (0x570B50): usercall(ESI=layer_ctx) + stdcall(4 params).
///
/// ESI must hold the display layer context (from DisplayGfx::set_active_layer).
/// The function uses ESI for LoadSpriteFromVfs and GfxResource__Create_Maybe
/// when param4 (gfx_dir) is non-null.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn wa_load_sprites(
    _wrapper: *mut DDGameWrapper,
    _sprite_data: *mut u8,
    _display_flags: u32,
    _param4: u32,
    _layer_ctx: *mut crate::render::palette::PaletteContext, // → ESI
) {
    core::arch::naked_asm!(
        "push esi",
        "mov esi, [esp+24]",  // layer_ctx (5th param: 4 saved + 4 ret + 4*4 params + 4 = 24)
        "push [esp+20]",      // param4
        "push [esp+20]",      // display_flags
        "push [esp+20]",      // sprite_data
        "push [esp+20]",      // wrapper
        "call [{addr}]",
        "pop esi",
        "ret",
        addr = sym GFX_LOAD_SPRITES_ADDR,
    );
}

static mut GFX_LOAD_SPRITES_ADDR: u32 = 0;

/// Port of DSSound_LoadEffectWAVs (0x5714B0).
///
/// Iterates the sound effect name table at 0x6AF378 (pairs of [slot_id, name_ptr],
/// null-terminated). For each entry, builds "data\wav\Effects\{name}.wav" and calls
/// DSSound::load_wav (vtable slot 12, already Rust).
unsafe fn load_effect_wavs(wrapper: *mut DDGameWrapper) {
    use crate::audio::dssound::load_wav;
    use std::ffi::CStr;

    /// Sound effect name table entry: (slot_id, name_ptr).
    #[repr(C)]
    struct SfxEntry {
        slot_id: i32,
        name_ptr: *const std::ffi::c_char,
    }

    let table = rb(0x006A_F378) as *const SfxEntry;
    let sound = (*wrapper).sound;
    if sound.is_null() {
        return;
    }

    let mut i = 0;
    loop {
        let entry = &*table.add(i);
        if entry.name_ptr.is_null() {
            break;
        }
        if entry.slot_id == 0 {
            break;
        }

        let name = CStr::from_ptr(entry.name_ptr).to_str().unwrap_or("");
        let path = format!("data\\wav\\Effects\\{}.wav\0", name);

        load_wav(sound, entry.slot_id, path.as_ptr());

        // Update loading progress bar
        call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);

        i += 1;
    }
}

/// PCLandscape__Constructor (0x57ACB0): construct landscape object (0xB44 bytes, 11 params).
///
/// param_5 is `game_info + 0xDAAC` (landscape data path region within GameInfo).
/// The SoundEmitter sub-constructor reads paths, water settings, etc. from this
/// pointer with negative offsets back into GameInfo.
#[cfg(target_arch = "x86")]
unsafe fn wa_pc_landscape_ctor(
    this: *mut u8,
    ddgame: *mut DDGame,
    gfx_resource: *mut u8,
    display: *mut DisplayGfx,
    landscape_data: *const u8,
    byte_output: *mut u8,
    gfx_mode: u32,
    temp_buf: *mut u32,
    coord_output: *mut u32,
    width_output: *mut u32,
    height_output: *mut u32,
) -> *mut u8 {
    let f: unsafe extern "stdcall" fn(
        *mut u8,
        *mut DDGame,
        *mut u8,
        *mut DisplayGfx,
        *const u8,
        *mut u8,
        u32,
        *mut u32,
        *mut u32,
        *mut u32,
        *mut u32,
    ) -> *mut u8 = core::mem::transmute(rb(va::PC_LANDSCAPE_CONSTRUCTOR) as usize);
    f(
        this,
        ddgame,
        gfx_resource,
        display,
        landscape_data,
        byte_output,
        gfx_mode,
        temp_buf,
        coord_output,
        width_output,
        height_output,
    )
}

/// DisplayGfx__Constructor (0x4F5E80): wrap raw image data in DisplayGfx object.
#[cfg(target_arch = "x86")]
unsafe fn wa_displaygfx_ctor(raw_image: *mut u8) -> *mut u8 {
    let f: unsafe extern "stdcall" fn(*mut u8) -> *mut u8 =
        core::mem::transmute(rb(va::DISPLAYGFX_CONSTRUCTOR) as usize);
    f(raw_image)
}

/// DDGame__InitDisplayFinal (0x56A830): finalize display for non-headless mode.
#[cfg(target_arch = "x86")]
unsafe fn wa_init_display_final(display: *mut DisplayGfx) {
    let f: unsafe extern "stdcall" fn(*mut DisplayGfx) =
        core::mem::transmute(rb(va::DDGAME_INIT_DISPLAY_FINAL) as usize);
    f(display);
}

/// DDGame__LoadHudAndWeaponSprites (0x53D0E0): load weapon icons and HUD sprites.
/// thiscall(ECX=gfx_dir) + 2 stack(ddgame, secondary_gfx_dir), RET 0x8.
#[cfg(target_arch = "x86")]
unsafe fn wa_load_hud_sprites(gfx_dir: *mut GfxDir, ddgame: *mut DDGame, secondary: *mut GfxDir) {
    let f: unsafe extern "thiscall" fn(*mut GfxDir, *mut DDGame, *mut GfxDir) =
        core::mem::transmute(rb(va::DDGAME_LOAD_HUD_AND_WEAPON_SPRITES) as usize);
    f(gfx_dir, ddgame, secondary);
}

/// DDGame__InitPaletteGradientSprites (0x5706D0): creates DisplayGfx palette
/// gradient objects for each team. stdcall(wrapper), RET 0x4.
#[cfg(target_arch = "x86")]
unsafe fn wa_init_palette_gradient_sprites(wrapper: *mut DDGameWrapper) {
    let f: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
        core::mem::transmute(rb(va::DDGAME_INIT_PALETTE_GRADIENT_SPRITES) as usize);
    f(wrapper);
}

/// Optional callback invoked right after DDGame allocation (before any field
/// initialization). Used by the hardware watchpoint debugger to arm DR0–DR3.
pub static mut ON_DDGAME_ALLOC: Option<unsafe fn(*mut u8)> = None;

/// Create and initialize DDGame, matching DDGame__Constructor (0x56E220).
///
/// Allocates 0x98D8 bytes from WA's heap, initializes all fields, and creates
/// sub-objects. Populates fields on both `wrapper` (DDGameWrapper) and the
/// returned DDGame.
///
/// # Safety
/// All pointer params must be valid WA objects. `wrapper` must be a
/// partially-initialized DDGameWrapper (vtable, display, sound set).
#[cfg(target_arch = "x86")]
pub unsafe fn create_ddgame(
    wrapper: *mut DDGameWrapper,
    keyboard: *mut DDKeyboard,
    display: *mut DisplayGfx,
    sound: *mut DSSound,
    palette: *mut Palette,
    music: *mut Music,
    param7: *mut u8,   // timer object (0x1F4 observed)
    net_game: *mut u8, // from GameSession
    game_info: *mut GameInfo,
    network_ecx: u32, // implicit ECX from caller
) -> *mut DDGame {
    // ── 1. Allocate and zero-fill ──
    // Original: malloc(0x98D8), memset(ptr, 0, 0x98B8) — last 0x20 bytes not zeroed.
    // We zero the full 0x98D8 for safety (strictly more initialization than original).
    let ddgame = wa_malloc_zeroed(0x98D8) as *mut DDGame;
    if ddgame.is_null() {
        return core::ptr::null_mut();
    }

    // Notify watchpoint debugger (if active) so it can arm DR0–DR3.
    if let Some(cb) = ON_DDGAME_ALLOC {
        cb(ddgame as *mut u8);
    }

    // ── 2. InitFields — pure Rust (replaces usercall bridge) ──
    ddgame_init_fields(ddgame);

    // ── 3-4. Store params BEFORE exposing DDGame via wrapper ──
    // Critical: game_info must be set before wrapper->ddgame, because the
    // message pump (triggered by audio loading) can cause game tasks to
    // read ddgame->game_info. If game_info is null, they crash.
    (*ddgame).display = display;
    (*ddgame).sound = sound;
    (*ddgame).keyboard = keyboard;
    (*ddgame).palette = palette;
    (*ddgame).music = music;
    (*ddgame).timer_obj = param7;
    (*ddgame).network_ecx = network_ecx;
    (*ddgame).game_info = game_info;
    (*ddgame).net_game = net_game;

    // Now safe to expose — all fields that concurrent readers check are set.
    (*wrapper).ddgame = ddgame;

    // ── 5. Set g_GameInfo global ──
    *(rb(va::G_GAME_INFO) as *mut *mut GameInfo) = game_info;

    // ── 6. Sound available + always-1 flags ──
    let is_headless = (*game_info).headless_mode != 0;
    // sound_available enables loading progress bar, message pump, and sound during construction.
    (*ddgame).sound_available = if is_headless { 0 } else { 1 };
    (*ddgame).field_7efc = 1;

    // ── 7. Network bridge (online games only) ──
    (*wrapper).net_bridge = core::ptr::null_mut();

    if (*game_info).game_version == -2 {
        let bridge = wa_malloc_zeroed(core::mem::size_of::<NetBridge>() as u32) as *mut NetBridge;
        (*bridge).ddgame = ddgame;
        (*bridge).net_config_1 = (*game_info).net_config_1;
        (*bridge).net_config_2 = (*game_info).net_config_2;
        (*wrapper).net_bridge = bridge;
        if network_ecx != 0 {
            *((network_ecx as *mut u8).add(0x18) as *mut *mut NetBridge) = bridge;
        }
    }

    // ── 9. InitVersionFlags — sets DDGame+0x7E2E/0x7E2F/0x7E3F ──
    wa_init_version_flags(wrapper);

    // ── 10. GfxHandler, landscape, sprites, audio, resources ──
    init_graphics_and_resources(wrapper, game_info, net_game, display, is_headless);

    let _ = crate::log::log_line("[DDGame] create_ddgame complete");
    ddgame
}

/// Second half of the constructor: GfxHandler, landscape, sprites, audio, resources.
///
/// Second half of the constructor — initializes graphics, audio, landscape, and sprites.
#[cfg(target_arch = "x86")]
unsafe fn init_graphics_and_resources(
    wrapper: *mut DDGameWrapper,
    game_info: *mut GameInfo,
    _net_game: *mut u8,
    _display: *mut DisplayGfx,
    is_headless: bool,
) {
    let ddgame = (*wrapper).ddgame;
    use core::ffi::c_char;
    let fopen: unsafe extern "cdecl" fn(*const c_char, *const c_char) -> *mut u8 =
        core::mem::transmute(rb(va::WA_FOPEN) as usize);
    let gfx_dir_vtable = rb(va::GFX_DIR_VTABLE) as *const GfxDirVtable;

    // ── GfxDir #1 (primary) ──
    let gfx1 = GfxDir::alloc(gfx_dir_vtable);
    (*wrapper).primary_gfx_dir = gfx1;
    (*wrapper).secondary_gfx_dir = core::ptr::null_mut();

    // Build path list (order depends on headless + display_flags)
    let headless = (*game_info).headless_mode as i32;
    let display_flags = (*game_info).display_flags as i32;
    let paths: [&core::ffi::CStr; 3] = if headless != 0 {
        if display_flags == 0 {
            [
                c"data\\Gfx\\Gfx.dir",
                c"data\\Gfx\\Gfx0.dir",
                c"data\\Gfx\\Gfx1.dir",
            ]
        } else {
            [
                c"data\\Gfx\\Gfx.dir",
                c"data\\Gfx\\Gfx1.dir",
                c"data\\Gfx\\Gfx0.dir",
            ]
        }
    } else if display_flags == 0 {
        [
            c"data\\Gfx\\Gfx0.dir",
            c"data\\Gfx\\Gfx1.dir",
            c"data\\Gfx\\Gfx.dir",
        ]
    } else {
        [
            c"data\\Gfx\\Gfx1.dir",
            c"data\\Gfx\\Gfx0.dir",
            c"data\\Gfx\\Gfx.dir",
        ]
    };

    let mut gfx_loaded_idx = 0u32;
    for (i, path) in paths.iter().enumerate() {
        let fp = fopen(path.as_ptr(), c"rb".as_ptr());
        (*gfx1).file_handle = fp;
        if !fp.is_null()
            && call_gfx_load_dir(
                gfx1 as *mut u8,
                crate::render::sprite::gfx_dir::gfx_load_dir_addr(),
            ) != 0
        {
            gfx_loaded_idx = i as u32;
            break;
        }
        if i == 2 {
            panic!("DDGame: couldn't open any Gfx.dir");
        }
    }

    let headless_offset = if headless != 0 { 1u32 } else { 0u32 };
    (*wrapper).gfx_mode = if gfx_loaded_idx.wrapping_sub(headless_offset) < 2 {
        1
    } else {
        0
    };

    // ── GfxDir #2 (conditional — supplemental sprites for certain game versions) ──
    let game_version = (*game_info).game_version;
    let threshold = if (*wrapper).gfx_mode != 0 { 33 } else { -2i32 };
    if game_version < threshold {
        use crate::render::sprite::gfx_dir::gfx_load_dir_addr;

        let c_digit = if game_version > -3 { b'2' } else { b'1' };
        let mut gfx_c_path = *b"data\\Gfx\\GfxC_3_0.dir\0";
        gfx_c_path[14] = c_digit;

        let gfx2 = GfxDir::alloc(gfx_dir_vtable);
        (*wrapper).secondary_gfx_dir = gfx2;

        let fp = fopen(gfx_c_path.as_ptr().cast(), c"rb".as_ptr());
        (*gfx2).file_handle = fp;
        let load_addr = gfx_load_dir_addr();
        if fp.is_null() || call_gfx_load_dir(gfx2 as *mut u8, load_addr) == 0 {
            let fp2 = fopen(c"data\\Gfx\\Gfx.dir".as_ptr(), c"rb".as_ptr());
            (*gfx2).file_handle = fp2;
            if fp2.is_null() || call_gfx_load_dir(gfx2 as *mut u8, load_addr) == 0 {
                panic!("DDGame: couldn't open secondary Gfx.dir");
            }
        }
    }

    // ── Display palette setup (non-headless) ──
    if !is_headless {
        if *(rb(va::G_DISPLAY_MODE_FLAG) as *const c_char) == 0 {
            call_usercall_eax(wrapper, FUN_570A90_ADDR);
        }
        let disp = (*wrapper).display;
        let gfx_dir = (*wrapper).primary_gfx_dir;
        (*disp).set_layer_color(1, 0xFE);
        (*disp).load_sprite(1, 1, 0, gfx_dir, rb(va::STR_CDROM_SPR) as *const c_char);
        (*disp).set_layer_visibility(1, -100);

        // Palette slot range init (raw byte offsets into DisplayBase)
        let disp_raw = disp as *mut u8;
        let palette_range_ptr = *(disp_raw.add(0x3120) as *const *mut i16);
        if !palette_range_ptr.is_null() && *(disp_raw.add(0x3534) as *const i32) == 0 {
            let start = *palette_range_ptr as u32;
            let end = (*palette_range_ptr.add(1) as u32) + 1;
            if start < end {
                for i in start..end {
                    *(disp_raw.add(0x312C + i as usize * 4) as *mut u32) = 1;
                }
            }
            crate::wa_alloc::wa_free(*(disp_raw.add(0x3120) as *const *mut u8));
            *(disp_raw.add(0x3120) as *mut u32) = 0;
        }
    }

    // ── FUN_00570E20: usercall(ESI=wrapper), plain RET ──
    // Runs for all modes — headless vtable[4] is 0x5231E0 (same as headful).
    display_layer_color_init(wrapper);

    // ── Display vtable slot 5 (offset 0x14) ──
    // Original: CALL EAX (vtable[5]), saves return value in ESI for use as
    // the `output` parameter in the color-entries GfxResource__Create call below.
    let layer_ctx = (*(*ddgame).display).set_active_layer(1);

    // ── GfxDir color entries DDGame+0x730C..0x732C ──
    // Original logic: if gfx_mode!=0, try GfxResource__Create for colours.img.
    // If gfx_mode==0 OR resource creation fails, fall back to LoadSprites.
    // The fallback's 4th param is primary_gfx_dir when gfx_mode==0, or 0 on resource fail.
    if (*wrapper).gfx_mode != 0 {
        let res = gfx_resource_create(
            (*wrapper).primary_gfx_dir,
            rb(va::STR_COLOURS_IMG) as *const core::ffi::c_char,
            layer_ctx,
        );
        if !res.is_null() {
            let rvt = *(res as *const *const u32);
            let get_color: unsafe extern "thiscall" fn(*mut u8, u32, u32) -> u32 =
                core::mem::transmute(*rvt.add(4));
            for i in 0..9u32 {
                (*ddgame).gfx_color_table[i as usize] = get_color(res, i, 0);
            }
            let release: unsafe extern "thiscall" fn(*mut u8, u8) =
                core::mem::transmute(*rvt.add(3));
            release(res, 1);
        } else {
            // Resource creation failed — fallback with param4=0
            wa_load_sprites(
                wrapper,
                (*ddgame).gfx_sprite_data.as_mut_ptr(),
                (*game_info).display_flags,
                0,
                layer_ctx,
            );
        }
    } else {
        // gfx_mode==0 (headless): fallback LoadSprites with param4=primary_gfx_dir
        wa_load_sprites(
            wrapper,
            (*ddgame).gfx_sprite_data.as_mut_ptr(),
            (*game_info).display_flags,
            (*wrapper).primary_gfx_dir as u32,
            layer_ctx,
        );
    }

    // ── Secondary PaletteContext (DDGame+0x2C, conditional on secondary GfxDir) ──
    if !(*wrapper).secondary_gfx_dir.is_null() {
        let palette_ctx = wa_malloc_zeroed(0x70C) as *mut crate::render::palette::PaletteContext;
        (*palette_ctx).dirty_range_min = 1;
        (*palette_ctx).dirty_range_max = 0x5A;
        crate::render::palette::palette_context_init(palette_ctx);
        (*palette_ctx).dirty = 0;
        (*ddgame).secondary_palette_ctx = palette_ctx;
        // param4=0 so the ESI-dependent block is skipped; layer_ctx doesn't matter
        wa_load_sprites(
            wrapper,
            (*ddgame).gfx_sprite_data.as_mut_ptr(),
            (*game_info).display_flags,
            0,
            core::ptr::null_mut(),
        );
    }

    // ── DDGameWrapper field inits ──
    (*wrapper).loading_progress = 0;
    if is_headless {
        (*wrapper).loading_total = 0x2AD;
    } else {
        let team_count = (*game_info).speech_team_count as u32;
        (*wrapper).loading_total = team_count * 0x38 + 0x7E + 0x2AD;
    }
    (*wrapper).loading_last_pct = 0xFFFFFF9C; // -100: forces first progress bar update
    (*wrapper).speech_name_count = 0;

    // ── Audio init (non-headless + sound available) ──
    if !is_headless {
        crate::engine::ddgame_load_fonts::load_fonts(wrapper);
        if !(*ddgame).sound.is_null() {
            load_effect_wavs(wrapper);
            // DSSound_LoadAllSpeechBanks: the original is hooked to our Rust
            // replacement (speech.rs), so the usercall bridge calls our code.
            call_usercall_esi(wrapper, LOAD_SPEECH_BANKS_ADDR);
            // Allocate ActiveSoundTable (0x608 bytes)
            let ast = wa_malloc(0x608) as *mut ActiveSoundTable;
            core::ptr::write_bytes(ast as *mut u8, 0, 0x600);
            (*ast).ddgame = ddgame;
            (*ast).counter = 0;
            (*ddgame).active_sounds = ast;
        }
    }

    // ── GfxResource for masks.img ──
    // The original constructor uses a stack-local PaletteContext buffer (ESP+0x1A4)
    // initialized at the start with (word=1, word=0xFE, PaletteContext__Init).
    // This same buffer is reused as the output param for GfxResource__Create(masks.img).
    // We replicate this initialization here. Size 0x900 matches the stack allocation.
    let gfx_resource: *mut u8;
    {
        let gfx_dir = (*wrapper).primary_gfx_dir;
        let palette_ctx = wa_malloc_zeroed(0x900) as *mut crate::render::palette::PaletteContext;
        (*palette_ctx).dirty_range_min = 1;
        (*palette_ctx).dirty_range_max = 0xFE;
        crate::render::palette::palette_context_init(palette_ctx);
        gfx_resource = gfx_resource_create(
            gfx_dir,
            rb(va::STR_MASKS_IMG) as *const core::ffi::c_char,
            palette_ctx as *mut crate::render::palette::PaletteContext,
        );
    }

    // ── Dump GfxResource for A/B comparison ──
    if !gfx_resource.is_null() {
        let use_orig = std::env::var("OPENWA_USE_ORIG_CTOR").is_ok();
        let tag = if use_orig { "orig" } else { "rust" };
        // Dump GfxResource object + first sub-object
        let gr_data = core::slice::from_raw_parts(gfx_resource, 0x100);
        let _ = std::fs::write(format!("gfx_resource_{}.bin", tag), gr_data);
    }

    // ── PCLandscape (alloc 0xB44, stdcall 11 params) ──
    // Temporary output buffers for landscape coordinate data (used later for coord_list).
    // These were stack locals in the original code (aiStack_978, iStack_11f9).
    let mut landscape_coords_buf = [0u32; 0x400]; // coord output: pairs of (x, y)
    let mut landscape_byte_buf = [0u8; 0x100]; // byte output
    let mut landscape_temp = [0u32; 0x400]; // [0] = coord count, rest = temp data

    let landscape = {
        let alloc = wa_malloc_zeroed(0xB44);
        if !alloc.is_null() {
            let result = wa_pc_landscape_ctor(
                alloc,
                ddgame,
                gfx_resource,
                (*wrapper).display,
                (*game_info).landscape_data_path.as_ptr(),
                landscape_byte_buf.as_mut_ptr(),
                (*wrapper).gfx_mode,
                landscape_temp.as_mut_ptr(),
                landscape_coords_buf.as_mut_ptr(),
                &raw mut (*ddgame).level_width_raw,
                &raw mut (*ddgame).level_height_raw,
            );
            (*wrapper).landscape = result as *mut PCLandscape;
            (*ddgame).landscape = result as *mut PCLandscape;
            result
        } else {
            (*wrapper).landscape = core::ptr::null_mut();
            (*ddgame).landscape = core::ptr::null_mut();
            core::ptr::null_mut()
        }
    };

    // ── Collision BitGrid at DDGame+0x380 ──
    {
        let width = (*ddgame).level_width;
        let height = (*ddgame).level_height;
        (*ddgame).collision_grid = CollisionBitGrid::alloc(1, width, height);
    }

    // ── 8× SpriteRegion at DDGame+0x46C..0x488 ──
    // SpriteRegion__Constructor: fastcall(ECX, EDX) + 6 stack(this, p2, p3, p4, p5, p6), RET 0x18
    {
        // (array_index, ECX, EDX, p2, p3, p4, p5, p6=gfx_resource)
        // p6 is the GfxResource pointer returned by GfxResource__Create_Maybe.
        let gr = gfx_resource as u32;
        let regions: [(usize, u32, u32, u32, u32, u32, u32, u32); 8] = [
            (2, 0x37, 0x36, 0x2E, 0x24, 0x41, 0x2D, gr),
            (0, 0x30, 0x0C, 0x2D, 0x07, 0x34, 0x09, gr),
            (1, 0x11, 0x1A, 0x0D, 0x0A, 0x16, 0x13, gr),
            (3, 0x0C, 0x3D, 0x00, 0x20, 0x18, 0x33, gr),
            (4, 0x00, 0x01, 0x00, 0x00, 0x01, 0x00, gr),
            (6, 0x1A2, 0x1B, 0x173, 0x09, 0x1D8, 0x03, gr),
            (7, 0x1EF, 0x26, 0x1E5, 0x07, 0x1F9, 0x16, gr),
            (5, 0x2D, 0x08, 0x2D, 0x07, 0x2E, 0x07, gr),
        ];

        for &(idx, ecx, edx, p2, p3, p4, p5, p6) in &regions {
            let alloc = wa_malloc_zeroed(0x9C);
            let result = if !alloc.is_null() {
                call_sprite_region_ctor(alloc, ecx, edx, p2, p3, p4, p5, p6)
            } else {
                core::ptr::null_mut()
            };
            (*ddgame).sprite_regions[idx] = result;
        }
    }

    // ── Landscape property at DDGame+0x468 (PCLandscape vtable[0xB]) ──
    if !landscape.is_null() {
        let land_vt = *(landscape as *const *const u32);
        let get_val: unsafe extern "thiscall" fn(*mut u8) -> u32 =
            core::mem::transmute(*land_vt.add(0xB));
        (*ddgame).landscape_property = get_val(landscape);
    }

    // NOTE: gfx_resource is NOT released here — arrow SpriteRegions need it.
    // It's released after the arrow loop below.

    // ── Arrow sprites + collision regions (32 iterations) ──
    {
        let gfx_dir = (*wrapper).primary_gfx_dir;

        for i in 0u32..32 {
            // Format "arrow%02u.img\0" into stack buffer
            let mut name_buf = *b"arrow00.img\0\0\0\0\0";
            name_buf[5] = b'0' + (i / 10) as u8;
            name_buf[6] = b'0' + (i % 10) as u8;

            let layer_ctx = (*(*ddgame).display).set_active_layer(1);

            let entry = gfx_dir_find_entry(name_buf.as_ptr().cast(), gfx_dir);

            let sprite: *mut u8;
            if !entry.is_null() {
                let entry_val = *(entry.add(4) as *const u32);
                let cached = GfxDir::load_cached_raw(gfx_dir, entry_val);
                if !cached.is_null() {
                    sprite = wa_displaygfx_ctor(cached);
                } else {
                    // Fallback: load from file via GfxDir__LoadImage + IMG_Decode
                    sprite = call_gfx_load_and_wrap(gfx_dir, name_buf.as_ptr().cast(), layer_ctx);
                }
            } else {
                // Entry not found — try direct file load
                sprite = call_gfx_load_and_wrap(gfx_dir, name_buf.as_ptr().cast(), layer_ctx);
            }

            // Store arrow sprite at DDGame+0x38+i*4
            (*ddgame).arrow_sprites[i as usize] = sprite;

            // Calculate collision region dimensions from sprite.
            // The original creates a CENTERED collision box with 10px margin:
            //   left/top margin = max(0, dim/2 - 10)
            //   right/bottom = dim - margin
            // Result: this[5]=this[6]=10 for sprites >20px (the 10px inset).
            if !sprite.is_null() {
                let grid = &*(sprite as *const BitGrid);
                let sprite_w = grid.width as i32;
                let sprite_h = grid.height as i32;
                let margin_w = (sprite_w / 2 - 10).max(0);
                let margin_h = (sprite_h / 2 - 10).max(0);

                // Create SpriteRegion for collision
                let alloc = wa_malloc_zeroed(0x9C);
                let region = if !alloc.is_null() {
                    call_sprite_region_ctor(
                        alloc,
                        (sprite_w / 2) as u32, // ECX → this[5] = ECX - p2
                        (sprite_h - margin_h) as u32, // EDX → this[4] = EDX - p3
                        margin_w as u32,       // p2 (left margin)
                        margin_h as u32,       // p3 (top margin)
                        (sprite_w - margin_w) as u32, // p4 → this[3] = p4 - p2
                        (sprite_h / 2) as u32, // p5 → this[6] = p5 - p3
                        sprite as u32,         // p6 = arrow sprite (NOT landscape gfx_resource)
                    )
                } else {
                    core::ptr::null_mut()
                };
                (*ddgame).arrow_collision_regions[i as usize] = region;
            }

            // Arrow GfxDir (conditional on secondary gfxdir)
            if !(*ddgame).secondary_palette_ctx.is_null() {
                let gfx_resource_create: unsafe extern "thiscall" fn(
                    *mut GfxDir,
                    *mut u8,
                ) -> *mut u8 = core::mem::transmute(rb(va::GFX_RESOURCE_CREATE) as usize);
                (*ddgame).arrow_gfxdirs[i as usize] =
                    gfx_resource_create(gfx_dir, core::ptr::null_mut());
            }
        }
    }

    // Release gfx_resource AFTER arrow loop (arrows need it for SpriteRegions)
    // vtable[3] = DisplayGfx__vmethod_3: thiscall(this, byte param_2), RET 4.
    if !gfx_resource.is_null() {
        let rvt = *(gfx_resource as *const *const u32);
        let release: unsafe extern "thiscall" fn(*mut u8, u8) = core::mem::transmute(*rvt.add(3));
        release(gfx_resource, 1);
    }

    // ── Display BitGrid at DDGame+0x138 ──
    (*ddgame).display_bitgrid = DisplayBitGrid::alloc(8, 0x100, 0x1E0);

    // ── CoordList at DDGame+0x50C (capacity 600, 0x12C0 buffer) ──
    {
        use crate::wa_alloc::wa_malloc_struct_zeroed;

        let cl = wa_malloc_struct_zeroed::<CoordList>();
        (*cl).count = 0;
        (*cl).capacity = 600;
        let data = wa_malloc_zeroed(600 * core::mem::size_of::<CoordListEntry>() as u32)
            as *mut CoordListEntry;
        (*cl).data = data;
        (*ddgame).coord_list = cl;

        // Populate coord_list from PCLandscape's coordinate output.
        // landscape_temp[0] = coordinate count, landscape_coords_buf = pairs of (x, y).
        // Original packs as: coord = x * 0x10000 + y (Fixed-point).
        // Duplicates are skipped.
        let coord_count = landscape_temp[0];
        for j in 0..coord_count as usize {
            let x = landscape_coords_buf[j * 2];
            let y = landscape_coords_buf[j * 2 + 1];
            let coord_val = x.wrapping_mul(0x10000).wrapping_add(y);
            let cur_count = (*cl).count as usize;
            if cur_count >= 600 {
                break;
            }
            // Check for duplicates
            let mut dup = false;
            for k in 0..cur_count {
                if (*data.add(k)).coord == coord_val {
                    dup = true;
                    break;
                }
            }
            if !dup {
                (*data.add(cur_count)).coord = coord_val;
                (*data.add(cur_count)).flag = 1;
                (*cl).count = (cur_count + 1) as u32;
            }
        }
    }

    // Temporary landscape buffers (stack arrays) are dropped automatically here.

    // ── Loading progress ticks (2 of 4 — before load_resource_list) ──
    call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);
    call_usercall_ecx(wrapper, LOADING_PROGRESS_TICK_ADDR);

    // ── Sprite resource loading via DDGameWrapper vtable[0] ──
    // DDNetGameWrapper__LoadResourceList: thiscall(ECX=wrapper) +
    // 5 stack params (layer, gfx_dir, base_path, data_table, table_size)
    {
        let landscape_ptr = (*wrapper).landscape;
        let water_dir = (*landscape_ptr).water_gfx_dir;
        let land_dir = (*landscape_ptr).level_gfx_dir;
        let gfx_dir = (*wrapper).primary_gfx_dir;

        let wrapper_vt = *(wrapper as *const *const u32);
        let load_resource_list: unsafe extern "thiscall" fn(
            *mut DDGameWrapper,
            u32,
            *mut GfxDir,
            *const u8,
            *const u8,
            u32,
        ) = core::mem::transmute(*wrapper_vt);
        // Load resources for layer 1 (main sprites)
        load_resource_list(
            wrapper,
            1,
            gfx_dir,
            rb(va::SPRITE_RESOURCE_BASE_PATH) as *const u8, // base path
            rb(va::SPRITE_RESOURCE_TABLE_1) as *const u8,
            0x1D88, // table size
        );
        // Set global flag based on game version
        let gv = (*(*ddgame).game_info).game_version;
        *(rb(va::G_SPRITE_VERSION_FLAG) as *mut u32) = if gv < 8 { 0 } else { 0x10 };

        // Load resources for layer 1 with different table
        load_resource_list(
            wrapper,
            1,
            gfx_dir,
            rb(va::SPRITE_RESOURCE_BASE_PATH) as *const u8,
            rb(va::SPRITE_RESOURCE_TABLE_2) as *const u8,
            0x18,
        );

        // Load resources for layer 2 (water)
        load_resource_list(
            wrapper,
            2,
            water_dir,
            rb(va::SPRITE_RESOURCE_BASE_PATH) as *const u8,
            rb(va::WATER_RESOURCE_TABLE) as *const u8,
            0x2F4,
        );

        let disp = (*wrapper).display;
        (*disp).set_active_layer(3);

        // back.spr and debris.spr must be loaded unconditionally — they're used by
        // GenerateDebrisParticles (0x546F70) for particle effects, which affects
        // the game RNG (DDGame+0x45EC). The original constructor loads them even
        // in headless mode. Skipping them causes replay desync.
        (*disp).load_sprite_by_layer(3, 0x26D, land_dir, c"back.spr".as_ptr().cast());
        // debris.spr must be loaded unconditionally — it's used by
        // GenerateDebrisParticles (0x546F70) for particle effects, which
        // affects the game RNG (DDGame+0x45EC). Skipping it in headless
        // mode causes desync (longbow replay checksum mismatch at frame 1350).
        (*disp).load_sprite(3, 0x26E, 0, land_dir, c"debris.spr".as_ptr());

        (*disp).load_sprite_by_layer(
            2,
            0x26C,
            water_dir,
            c"layer\\layer.spr|layer.spr".as_ptr().cast(),
        );

        (*ddgame).gradient_image_2 = core::ptr::null_mut();

        // ── Gradient image (0x030) ──
        let level_height = (*ddgame).level_height as i32;
        let layer3_ctx = (*disp).set_active_layer(3);
        let s_var1 = (*layer3_ctx).cache_count;

        if s_var1 < 0x61 && level_height == 0x2B8 {
            // Simple gradient: load gradient.img directly
            let gradient = call_gfx_find_and_load(land_dir, c"gradient.img", layer3_ctx);
            (*ddgame).gradient_image = gradient;
        } else {
            compute_complex_gradient(ddgame, land_dir, layer3_ctx, s_var1);
        }

        // ── Fill image → fill_pixel (0x7338) ──
        {
            let layer2_ctx = (*(*ddgame).display).set_active_layer(2);
            // In the original, fill.img uses piStack_126c which the decompiler
            // shows was set from piVar3 (water_layer from landscape+0xB38).
            let fill_sprite = call_gfx_find_and_load(water_dir, c"fill.img", layer2_ctx);
            if !fill_sprite.is_null() {
                // Get pixel value: fill_sprite->vtable[4](0, 0)
                let fill_vt = *(fill_sprite as *const *const u32);
                let get_pixel: unsafe extern "thiscall" fn(*mut u8, i32, i32) -> u32 =
                    core::mem::transmute(*fill_vt.add(4));
                (*ddgame).fill_pixel = get_pixel(fill_sprite, 0, 0);
                // Release fill sprite: vtable[3] = DisplayGfx__vmethod_3(this, param_2=1)
                let release: unsafe extern "thiscall" fn(*mut u8, u8) =
                    core::mem::transmute(*fill_vt.add(3));
                release(fill_sprite, 1);
            }
        }

        // ── DDGame__LoadHudAndWeaponSprites (0x53D0E0) ──
        wa_load_hud_sprites(gfx_dir, ddgame, (*wrapper).secondary_gfx_dir);

        // ── DDGame__InitPaletteGradientSprites (0x5706D0) ──
        // Creates DisplayGfx objects at DDGame+0x41C.. for each team's palette
        // gradient data from GameInfo. stdcall(wrapper), RET 0x4.
        wa_init_palette_gradient_sprites(wrapper);

        // ── DisplayGfx__InitTeamPaletteDisplayObjects (0x5703E0) ──
        // Creates team palette gradient display objects. Reads DDGame+0x7338
        // (fill_pixel), creates BitGrid+DisplayGfx per team.
        // stdcall(wrapper), RET 0x4.
        let init_team_palette_display: unsafe extern "stdcall" fn(*mut DDGameWrapper) =
            core::mem::transmute(rb(va::DISPLAY_GFX_INIT_TEAM_PALETTE_DISPLAY));
        init_team_palette_display(wrapper);
    }
    // ── Gradient image stub (DDGame+0x30) ──
    // Minimal stub: [6]=0 (zero-width) so CTaskLand skips the gradient column loop.
    if (*ddgame).gradient_image.is_null() {
        use crate::wa_alloc::wa_malloc_struct_zeroed;

        let obj = wa_malloc_struct_zeroed::<BitGrid>();
        if !obj.is_null() {
            (*obj).vtable = rb(va::BIT_GRID_BASE_VTABLE) as *const BitGridBaseVtable;
            // height = 0 → CTaskLand skips the gradient column loop
            (*ddgame).gradient_image = obj as *mut u8;
        }
    }

    // ── Release primary GfxHandler (vtable[3] = release, param 1 = free) ──
    let primary_gfx_dir = (*wrapper).primary_gfx_dir;
    if !primary_gfx_dir.is_null() {
        GfxDir::release_raw(primary_gfx_dir, 1);
    }

    if !is_headless {
        wa_init_display_final((*wrapper).display);
    }

    // ── FUN_00570A90 (second call, conditional) ──
    if *(rb(va::G_DISPLAY_MODE_FLAG) as *const core::ffi::c_char) == 0 {
        call_usercall_eax(wrapper, FUN_570A90_ADDR);
    }

    // ── Final display layer visibility ──
    {
        let disp = (*wrapper).display;
        (*disp).set_layer_visibility(1, 0);
        (*disp).set_layer_visibility(2, 0);
        (*disp).set_layer_visibility(3, 1);
    }

    let _ = crate::log::log_line("[DDGame] init_graphics_and_resources DONE");
}

// Statics for usercall bridge addresses
static mut FUN_570A90_ADDR: u32 = 0;

static mut LOAD_SPEECH_BANKS_ADDR: u32 = 0;
static mut LOADING_PROGRESS_TICK_ADDR: u32 = 0;

/// Bridge: usercall(ESI=wrapper), plain RET. Used by FUN_570E20, LoadSpeechBanks.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_usercall_esi(_wrapper: *mut DDGameWrapper, _addr: u32) {
    core::arch::naked_asm!(
        "pushl %esi",
        "movl 8(%esp), %esi",  // ESI = wrapper
        "movl 12(%esp), %eax", // EAX = target address
        "calll *%eax",
        "popl %esi",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(EAX=wrapper), plain RET. Used by FUN_570A90.
#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_usercall_eax(_wrapper: *mut DDGameWrapper, _addr: u32) {
    core::arch::naked_asm!(
        "movl 4(%esp), %eax", // EAX = wrapper
        "movl 8(%esp), %ecx", // ECX = target address (temp)
        "calll *%ecx",
        "retl",
        options(att_syntax),
    );
}

/// Bridge: usercall(ECX=wrapper), plain RET. Used by FUN_5717A0.
#[cfg(target_arch = "x86")]
unsafe fn call_usercall_ecx(wrapper: *mut DDGameWrapper, addr: u32) {
    let f: unsafe extern "thiscall" fn(*mut DDGameWrapper) = core::mem::transmute(addr as usize);
    f(wrapper);
}

/// Bridge to SpriteRegion__Constructor (0x57DB20).
/// Convention: fastcall(ECX, EDX) + 6 stack params, RET 0x18.
#[cfg(target_arch = "x86")]
static mut SPRITE_REGION_CTOR_ADDR: u32 = 0;

#[cfg(target_arch = "x86")]
#[unsafe(naked)]
unsafe extern "C" fn call_sprite_region_ctor(
    _this: *mut u8,
    _ecx: u32,
    _edx: u32,
    _p2: u32,
    _p3: u32,
    _p4: u32,
    _p5: u32,
    _p6: u32,
) -> *mut u8 {
    // Stack on entry: [ret] [this] [ecx] [edx] [p2] [p3] [p4] [p5] [p6]
    // Need: ECX=ecx, EDX=edx, push p6 p5 p4 p3 p2 this, call
    // SpriteRegion__Constructor: fastcall(ECX, EDX) + 6 stack, RET 0x18
    // Params: (this, ecx, edx, p2, p3, p4, p5, p6)
    core::arch::naked_asm!(
        "pushl %ebp",
        "pushl %ebx",
        // Load ECX and EDX (shifted by 2 pushes = 8 bytes)
        "movl 16(%esp), %ecx",   // ecx param (offset 4+8=12? No: 0=ebx,4=ebp,8=ret,12=this,16=ecx)
        "movl 20(%esp), %edx",   // edx param
        // Push 6 stack params in reverse: p6, p5, p4, p3, p2, this
        "pushl 40(%esp)",         // p6 (0=ebx,4=ebp,8=ret,...,40=p6)
        "pushl 40(%esp)",         // p5 (shifted by 4)
        "pushl 40(%esp)",         // p4 (shifted by 8)
        "pushl 40(%esp)",         // p3 (shifted by 12)
        "pushl 40(%esp)",         // p2 (shifted by 16)
        "pushl 32(%esp)",         // this (shifted by 20)
        "calll *({addr})",        // fastcall, callee cleans 6×4=24 bytes
        "popl %ebx",
        "popl %ebp",
        "retl",
        addr = sym SPRITE_REGION_CTOR_ADDR,
        options(att_syntax),
    );
}
