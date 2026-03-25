/// MapView — CWnd-derived terrain/map preview and data window (0x29628 bytes).
///
/// Inherits: CWnd → FUN_00425ff0 (vtable 0x64C348) → MapView (vtable 0x64FC60).
/// Secondary vtable at offset +0x54 (0x64FD94).
///
/// A general-purpose map class used across all game modes — not replay-specific.
/// Embedded as a child window in frontend dialog classes:
/// - FrontendLocalMP at offset 0x588 (param_1 + 0x162)
/// - FrontendCampaignA at offset 0x140 (param_1 + 0x50)
/// - FrontendNetworkLobby, FrontendNetworkChat (similar embedding)
///
/// In the replay loader, constructed transiently to parse a .thm terrain file
/// and extract map info via `MapView__CopyInfo` (0x449B60).
///
/// Key functions:
/// - Constructor: 0x447E80 — stdcall(alloc, flags), RET 0x8
/// - Load: 0x44A9A0 — stdcall(this, path, flags), RET 0xC
/// - CopyInfo: 0x449B60 — usercall(ESI=this), plain RET
///
/// Loads `.thm` terrain files (`data\current.thm` for live games,
/// `data\playback.thm` for replays).
///
/// PARTIAL: Only the vtable and terrain_flag field are mapped.
#[repr(C)]
pub struct MapView {
    /// 0x00000: Vtable pointer (0x64FC60 at image base).
    pub vtable: *const MapViewVtable,
    /// 0x00004-0x29617: Unknown (CWnd fields, map/terrain data, pixel grids, etc.).
    pub _unknown_004: [u8; 0x29618 - 4],
    /// 0x29618: Terrain type flag. Zero = cavern terrain (SETZ in replay loader).
    pub terrain_flag: u8,
    /// 0x29619-0x29627: Unknown trailing bytes.
    pub _unknown_29619: [u8; 0x29628 - 0x29619],
}

const _: () = assert!(core::mem::size_of::<MapView>() == 0x29628);

/// MapView vtable (partial — only known slots).
#[openwa_core::vtable(size = 2, va = 0x0064_FC60, class = "MapView")]
pub struct MapViewVtable {
    /// Destructor — thiscall(this, free_flag). Frees the object if flag & 1.
    #[slot(1)]
    pub destructor: fn(this: *mut MapView, free_flag: i32),
}
