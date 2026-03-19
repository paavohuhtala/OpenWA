//! Rust replacements for DDGame constructor sub-functions.
//!
//! Each function is hooked individually so it works regardless of whether the
//! DDGame constructor itself is Rust or the original WA code.
//!
//! ## Ported functions
//!
//! - `DDGame__InitFields` (0x526120): usercall(EDI=ddgame), plain RET → EAX=ddgame
//! - `DDGame__InitRenderIndices` (0x526080): usercall(ESI=base), plain RET → EAX=base
//! - `BitGrid__Init` (0x4F6370): usercall(ESI,ECX,EDI) + 1 stack, RET 0x4
//! - `GfxResource__Create_Maybe` (0x4F6300): usercall(ECX,EAX) + 1 stack, RET 0x4
//! - `FUN_570E20` (display layer init): usercall(ESI=wrapper), plain RET

use crate::hook;
use core::ffi::c_char;
use openwa_core::address::va;
use openwa_core::engine::ddgame::{
    bit_grid_init, ddgame_init_fields, ddgame_init_render_indices, display_layer_color_init,
    gfx_dir_find_entry, gfx_dir_load_dir, gfx_resource_create, DDGame,
};
use openwa_core::engine::game_state_init::{
    init_alliance_data, init_team_scoring, ring_buffer_init, sprite_gfx_table_init,
};
use openwa_core::engine::DDGameWrapper;

// ─── DDGame__InitFields (0x526120) ──────────────────────────────────────────

hook::usercall_trampoline!(
    fn init_fields_trampoline;
    impl_fn = impl_init_fields;
    reg = edi
);

extern "cdecl" fn impl_init_fields(ddgame: u32) -> u32 {
    unsafe { ddgame_init_fields(ddgame as *mut DDGame) }
    ddgame // Original: MOV EAX, EDI; RET
}

// ─── DDGame__InitRenderIndices (0x526080) ───────────────────────────────────

hook::usercall_trampoline!(
    fn init_render_indices_trampoline;
    impl_fn = impl_init_render_indices;
    reg = esi
);

extern "cdecl" fn impl_init_render_indices(base: u32) -> u32 {
    unsafe { ddgame_init_render_indices(base as *mut u8) }
    base // Original: MOV EAX, ESI; RET
}

// ─── BitGrid__Init (0x4F6370) ──────────────────────────────────────

extern "cdecl" fn impl_tsm_init(object: u32, param1: u32, height: u32, width: u32) -> u32 {
    unsafe { bit_grid_init(object as *mut u8, param1, width, height) }
    object // Original: MOV EAX, ESI; RET 0x4
}

#[unsafe(naked)]
unsafe extern "C" fn tsm_init_trampoline() {
    core::arch::naked_asm!(
        "push edx",
        "push [esp+8]",       // width (stack param)
        "push edi",           // height
        "push ecx",           // param1
        "push esi",           // object
        "call {impl_fn}",
        "add esp, 16",
        "pop edx",
        "ret 0x4",
        impl_fn = sym impl_tsm_init,
    );
}

// ─── FUN_570E20 (display layer color init) ──────────────────────────────────

hook::usercall_trampoline!(
    fn display_layer_init_trampoline;
    impl_fn = impl_display_layer_init;
    reg = esi
);

extern "cdecl" fn impl_display_layer_init(wrapper: u32) -> u32 {
    unsafe { display_layer_color_init(wrapper as *mut DDGameWrapper) }
    wrapper
}

// ─── GfxResource__Create_Maybe (0x4F6300) ───────────────────────────────────

extern "cdecl" fn impl_gfx_resource_create(gfx_dir: u32, name: u32, output: u32) -> u32 {
    let result = unsafe {
        gfx_resource_create(gfx_dir as *mut u8, name as *const c_char, output as *mut u8)
    };
    result as u32
}

#[unsafe(naked)]
unsafe extern "C" fn gfx_resource_create_trampoline() {
    core::arch::naked_asm!(
        "push edx",
        "push [esp+8]",       // output (stack param)
        "push eax",           // name
        "push ecx",           // gfx_dir
        "call {impl_fn}",
        "add esp, 12",
        "pop edx",
        "ret 0x4",
        impl_fn = sym impl_gfx_resource_create,
    );
}

// ─── GfxDir__FindEntry (0x566520) ────────────────────────────────────────────
//
// Convention: usercall(EAX=name) + 1 stack(gfx_dir), RET 0x4.

hook::usercall_trampoline!(
    fn find_entry_trampoline;
    impl_fn = impl_find_entry;
    reg = eax;
    stack_params = 1; ret_bytes = "0x4"
);

static FIND_ENTRY_LOG_COUNT: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(0);

extern "cdecl" fn impl_find_entry(name: u32, gfx_dir: u32) -> u32 {
    let result = unsafe { gfx_dir_find_entry(name as *const c_char, gfx_dir as *mut u8) };

    // Log first 20 lookups for debugging
    let count = FIND_ENTRY_LOG_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    if count < 20 {
        let name_str = unsafe {
            let p = name as *const u8;
            let mut len = 0;
            while *p.add(len) != 0 && len < 64 {
                len += 1;
            }
            core::str::from_utf8_unchecked(core::slice::from_raw_parts(p, len))
        };
        let _ = crate::log_line(&format!(
            "[FindEntry] #{count}: \"{name_str}\" -> 0x{:08X}",
            result as u32
        ));
    }
    result as u32
}

// ─── GfxHandler__LoadDir (0x5663E0) ──────────────────────────────────────────
//
// Convention: usercall(EAX=handler), plain RET. Returns 1/0.

hook::usercall_trampoline!(
    fn load_dir_trampoline;
    impl_fn = impl_load_dir;
    reg = eax
);

extern "cdecl" fn impl_load_dir(handler: u32) -> u32 {
    unsafe { gfx_dir_load_dir(handler as *mut u8) as u32 }
}

// ─── Hook installation ──────────────────────────────────────────────────────

pub fn install() -> Result<(), String> {
    unsafe {
        hook::install(
            "DDGame__InitFields",
            va::DDGAME_INIT_FIELDS,
            init_fields_trampoline as *const (),
        )?;

        hook::install(
            "DDGame__InitRenderIndices",
            va::DDGAME_INIT_RENDER_INDICES,
            init_render_indices_trampoline as *const (),
        )?;

        hook::install(
            "BitGrid__Init",
            va::BIT_GRID_INIT,
            tsm_init_trampoline as *const (),
        )?;

        hook::install(
            "GfxResource__Create",
            va::GFX_RESOURCE_CREATE,
            gfx_resource_create_trampoline as *const (),
        )?;

        hook::install(
            "FUN_570E20_DisplayLayerInit",
            va::FUN_570E20,
            display_layer_init_trampoline as *const (),
        )?;

        hook::install(
            "GfxDir__FindEntry",
            va::GFX_DIR_FIND_ENTRY,
            find_entry_trampoline as *const (),
        )?;

        hook::install(
            "GfxHandler__LoadDir",
            va::GFX_DIR_LOAD_DIR,
            load_dir_trampoline as *const (),
        )?;

        hook::install(
            "SpriteGfxTable__Init",
            va::SPRITE_GFX_TABLE_INIT,
            sprite_gfx_table_init_trampoline as *const (),
        )?;

        hook::install(
            "RingBuffer__Init",
            va::RING_BUFFER_INIT,
            ring_buffer_init_trampoline as *const (),
        )?;

        hook::install(
            "CGameTask__InitTeamScoring",
            va::INIT_TEAM_SCORING,
            init_team_scoring_trampoline as *const (),
        )?;

        hook::install(
            "CGameTask__InitAllianceData",
            va::INIT_ALLIANCE_DATA,
            init_alliance_data_trampoline as *const (),
        )?;
    }

    Ok(())
}

// ─── SpriteGfxTable__Init (0x541620) ────────────────────────────────────────
// Convention: fastcall(ECX=base, EDX=count), plain RET.

unsafe extern "fastcall" fn sprite_gfx_table_init_trampoline(base: u32, count: u32) {
    sprite_gfx_table_init(base as *mut u8, count);
}

// ─── RingBuffer__Init (0x541060) ────────────────────────────────────────────
// Convention: usercall(EAX=capacity, ESI=struct_ptr), plain RET.

extern "cdecl" fn impl_ring_buffer_init(struct_ptr: u32, capacity: u32) {
    unsafe { ring_buffer_init(struct_ptr as *mut u8, capacity) }
}

#[unsafe(naked)]
unsafe extern "C" fn ring_buffer_init_trampoline() {
    core::arch::naked_asm!(
        "push edx",
        "push eax",        // capacity (EAX)
        "push esi",        // struct_ptr (ESI)
        "call {impl_fn}",
        "add esp, 8",
        "pop edx",
        "ret",
        impl_fn = sym impl_ring_buffer_init,
    );
}

// ─── CGameTask__InitTeamScoring (0x528510) ──────────────────────────────────
// Convention: fastcall(ECX=wrapper), plain RET.

unsafe extern "fastcall" fn init_team_scoring_trampoline(wrapper: u32, _edx: u32) {
    init_team_scoring(wrapper as *mut u8);
}

// ─── CGameTask__InitAllianceData (0x5262D0) ─────────────────────────────────
// Convention: usercall(EAX=wrapper), plain RET.

extern "cdecl" fn impl_init_alliance_data(wrapper: u32) {
    unsafe { init_alliance_data(wrapper as *mut u8) }
}

#[unsafe(naked)]
unsafe extern "C" fn init_alliance_data_trampoline() {
    core::arch::naked_asm!(
        "push edx",
        "push eax",        // wrapper (EAX)
        "call {impl_fn}",
        "add esp, 4",
        "pop edx",
        "ret",
        impl_fn = sym impl_init_alliance_data,
    );
}
