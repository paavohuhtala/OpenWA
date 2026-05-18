//! Rust replacements for GameWorld constructor sub-functions.
//!
//! Each function is hooked individually so it works regardless of whether the
//! GameWorld constructor itself is Rust or the original WA code. Hook
//! installation is driven by `crates/openwa-dll/hooks/world_init.toml` joined
//! against `re/**/*.toml`; the codegen emits typed signature guards, naked
//! trampolines (for `custom_storage`), and `install_*` helpers in
//! `crate::generated::hooks`.

use core::ffi::{c_char, c_void};

use openwa_core::weapon::WeaponId;
use openwa_game::asset::gfx_dir::{
    GfxDir, gfx_dir_find_entry, gfx_dir_load_dir, img_load_from_dir,
};
use openwa_game::bitgrid::BitGrid;
use openwa_game::engine::game_state_init::init_turn_state;
use openwa_game::engine::ring_buffer::ring_buffer_init;
use openwa_game::engine::team_init::{init_alliance_data, init_team_scoring};
use openwa_game::engine::{
    EntityActivityQueue, GameRuntime, GameWorld, display_layer_color_init, game_world_init_fields,
    game_world_init_render_indices,
};
use openwa_game::game::{check_weapon_avail, is_super_weapon};
use openwa_game::render::landscape::init_landscape_borders;

// ─── GameWorld__InitFields (0x526120): usercall(EDI=world), plain RET → EAX=world ──

pub(crate) unsafe extern "cdecl" fn impl_init_fields(world: *mut GameWorld) -> *mut GameWorld {
    unsafe { game_world_init_fields(world) }
    world
}

// ─── GameWorld__InitRenderIndices (0x526080): usercall(ESI=base), plain RET → EAX=base ──

pub(crate) unsafe extern "cdecl" fn impl_init_render_indices(base: u32) -> u32 {
    // ESI = world + 0x72D8; recover the GameWorld pointer.
    let world = (base - 0x72D8) as *mut GameWorld;
    unsafe { game_world_init_render_indices(world) }
    base
}

// ─── BitGrid__Init (0x4F6370): usercall(ESI=grid, ECX=cells, EDI=height) + stack(width), RET 0x4 ──

pub(crate) unsafe extern "cdecl" fn impl_bitgrid_init(
    bit_grid: *mut BitGrid,
    cells_per_unit: u32,
    height: u32,
    width: u32,
) -> *mut BitGrid {
    unsafe { BitGrid::init(bit_grid, cells_per_unit, width, height) }
    bit_grid
}

// ─── GameWorld__InitDisplayLayerColors (0x570E20): usercall(ESI=runtime), plain RET ──

pub(crate) unsafe extern "cdecl" fn impl_display_layer_init(
    runtime: *mut GameRuntime,
) -> *mut GameRuntime {
    unsafe { display_layer_color_init(runtime) }
    runtime
}

// ─── IMG__LoadFromDir (0x4F6300): usercall(ECX=gfx_dir, EAX=name) + stack(output), RET 0x4 ──

pub(crate) unsafe extern "cdecl" fn impl_img_load_from_dir(
    gfx_dir: *mut GfxDir,
    name: *mut c_char,
    output: *mut c_void,
) -> u32 {
    // The trampoline catches an opaque stack pointer; the underlying
    // WA caller always passes a `PaletteContext*` (verified at all known
    // call sites — `set_active_layer`'s return value).
    let output = output as *mut openwa_game::render::palette::PaletteContext;
    unsafe { img_load_from_dir(gfx_dir, name as *const c_char, output) as u32 }
}

// ─── GfxDir__FindEntry (0x566520): usercall(EAX=name) + stack(gfx_dir), RET 0x4 ──

pub(crate) unsafe extern "cdecl" fn impl_find_entry(
    name: *mut c_char,
    gfx_dir: *mut GfxDir,
) -> u32 {
    unsafe { gfx_dir_find_entry(name as *const c_char, gfx_dir) as u32 }
}

// ─── GfxDir__LoadDir (0x5663E0): usercall(EAX=handler), plain RET. Returns 1/0. ──

pub(crate) unsafe extern "cdecl" fn impl_load_dir(handler: *mut c_void) -> u32 {
    unsafe { gfx_dir_load_dir(handler as *mut u8) as u32 }
}

// ─── EntityActivityQueue__Init (0x541620): fastcall(ECX=this, EDX=capacity), plain RET ──

pub(crate) unsafe extern "fastcall" fn impl_entity_activity_queue_init(
    this: *mut EntityActivityQueue,
    capacity: u32,
) {
    unsafe { EntityActivityQueue::init(this, capacity) }
}

// ─── RingBuffer__Init (0x541060): usercall(EAX=capacity, ESI=struct_ptr), plain RET ──

pub(crate) unsafe extern "cdecl" fn impl_ring_buffer_init(struct_ptr: *mut c_void, capacity: u32) {
    unsafe { ring_buffer_init(struct_ptr as *mut u8, capacity) }
}

// ─── WorldEntity__InitTeamScoring (0x528510): fastcall(ECX=runtime), plain RET ──

pub(crate) unsafe extern "fastcall" fn impl_init_team_scoring(runtime: *mut GameRuntime) {
    unsafe { init_team_scoring(runtime) }
}

// ─── WorldEntity__InitAllianceData (0x5262D0): usercall(EAX=runtime), plain RET ──

pub(crate) unsafe extern "cdecl" fn impl_init_alliance_data(runtime: *mut GameRuntime) {
    unsafe { init_alliance_data(runtime) }
}

// ─── WorldEntity__InitTurnState (0x528690): usercall(EAX=runtime), plain RET ──

pub(crate) unsafe extern "cdecl" fn impl_init_turn_state(runtime: *mut GameRuntime) {
    unsafe { init_turn_state(runtime) }
}

// ─── GameWorld__CheckWeaponAvail (0x53FFC0): usercall(ECX=world, ESI=weapon_id), plain RET ──

pub(crate) unsafe extern "cdecl" fn impl_check_weapon_avail(
    world: *mut GameWorld,
    weapon_id: u32,
) -> i32 {
    unsafe { check_weapon_avail(world, WeaponId(weapon_id)) }
}

// ─── InitLandscapeBorders (0x528480): usercall(EAX=runtime), plain RET ──

pub(crate) unsafe extern "cdecl" fn impl_init_landscape_borders(runtime: *mut GameRuntime) {
    unsafe { init_landscape_borders(runtime) }
}

// ─── Weapon__is_super_weapon (0x565960): usercall(EAX=weapon_id) + stack(select_worm), RET 0x4 ──
//
// Preserves ECX across the cdecl impl call (callers can rely on it staying
// intact even though the ABI says caller-saved).

pub(crate) unsafe extern "cdecl" fn impl_is_super_weapon(
    weapon_id: u32,
    select_worm_is_super_weapon: u32,
) -> u32 {
    is_super_weapon(WeaponId(weapon_id), select_worm_is_super_weapon != 0) as u32
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        crate::generated::hooks::install_GameWorld__InitFields()?;
        crate::generated::hooks::install_GameWorld__InitRenderIndices()?;
        crate::generated::hooks::install_BitGrid__Init()?;
        crate::generated::hooks::install_IMG__LoadFromDir()?;
        crate::generated::hooks::install_GameWorld__InitDisplayLayerColors()?;
        crate::generated::hooks::install_GfxDir__FindEntry()?;
        crate::generated::hooks::install_GfxDir__LoadDir()?;
        crate::generated::hooks::install_EntityActivityQueue__Init()?;
        crate::generated::hooks::install_RingBuffer__Init()?;
        crate::generated::hooks::install_WorldEntity__InitTeamScoring()?;
        crate::generated::hooks::install_WorldEntity__InitAllianceData()?;
        crate::generated::hooks::install_WorldEntity__InitTurnState()?;
        crate::generated::hooks::install_GameWorld__CheckWeaponAvail()?;
        crate::generated::hooks::install_InitLandscapeBorders()?;
        crate::generated::hooks::install_Weapon__is_super_weapon()?;
    }

    Ok(())
}
