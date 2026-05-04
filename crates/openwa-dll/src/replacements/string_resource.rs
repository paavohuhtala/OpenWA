//! Full-replacement hook for `WA__LoadStringResource` (0x00593180).
//!
//! Routes every WA.exe caller that reaches the non-inlined 0x593180 entry
//! through the Rust port in `openwa_game::wa::string_resource`. WA.exe also
//! has inlined copies of the same logic scattered across other functions;
//! those still read the globals directly and are not caught by this hook.

use crate::hook;
use openwa_game::address::va;
use openwa_game::wa::string_resource::wa_load_string_detour;

pub fn install() -> Result<(), String> {
    unsafe {
        let _ = hook::install(
            "WA__LoadStringResource",
            va::WA_LOAD_STRING,
            wa_load_string_detour as *const (),
        )?;
    }
    Ok(())
}
