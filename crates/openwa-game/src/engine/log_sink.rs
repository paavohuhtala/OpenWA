//! Byte-oriented writer for WA's headless-log streams.
//!
//! WA logs are not UTF-8: localized team/scheme names arrive as bytes in
//! WA's internal encoding, and the game optionally recodes them through a
//! per-ACP LUT before writing. `LogOutput` captures that model as a tiny
//! sink so the rest of the engine can use `write_bytes` / `write_cstr` /
//! `write_u32` etc. instead of transmuting into the CRT `fprintf` /
//! `snprintf_s` variadic ABI.
//!
//! Two output paths, matching the original:
//!
//! - **Passthrough** (`codepage_recode_on() == false`): bytes go straight
//!   to the stream via the CRT `fputc` import.
//! - **Recoded** (recode flag on): bytes are translated via
//!   `LUT[byte + 0x100]` — a 512-byte table built lazily on first use by
//!   `Codepage__BuildLut(GetACP())` — then written. All ASCII passes
//!   through unchanged; only non-ASCII bytes in team/scheme strings shift.

use core::ffi::{c_char, c_void};

use windows_sys::Win32::Globalization::GetACP;

use crate::address::va;
use crate::rebase::rb;

// Resolved at DLL load by `init_log_sink_addrs`.
static mut CODEPAGE_BUILD_LUT_ADDR: u32 = 0;

pub unsafe fn init_log_sink_addrs() {
    unsafe {
        CODEPAGE_BUILD_LUT_ADDR = rb(va::CODEPAGE_BUILD_LUT);
    }
}

/// `Codepage__BuildLut` (0x00592280). Usercall(EAX=codepage) → LUT pointer.
#[unsafe(naked)]
unsafe extern "stdcall" fn bridge_codepage_build_lut(_acp: u32) -> *const u8 {
    core::arch::naked_asm!(
        "popl %ecx",
        "popl %eax",
        "pushl %ecx",
        "jmpl *({fn})",
        fn = sym CODEPAGE_BUILD_LUT_ADDR,
        options(att_syntax),
    );
}

#[inline(always)]
unsafe fn codepage_recode_on() -> bool {
    unsafe { *(rb(va::G_CODEPAGE_RECODE_FLAG) as *const u8) != 0 }
}

unsafe fn codepage_lut() -> *const u8 {
    unsafe {
        let slot = rb(va::G_CODEPAGE_LUT) as *mut u32;
        if *slot == 0 {
            let acp = GetACP();
            *slot = bridge_codepage_build_lut(acp) as u32;
        }
        *slot as *const u8
    }
}

#[inline(always)]
unsafe fn putc(b: u8, stream: *mut c_void) {
    unsafe {
        let putc_ptr = *(rb(va::CRT_PUTC_IAT) as *const u32) as usize;
        let f: unsafe extern "C" fn(i32, *mut c_void) -> i32 = core::mem::transmute(putc_ptr);
        f(b as i32, stream);
    }
}

unsafe fn strlen(p: *const c_char) -> usize {
    unsafe {
        let mut n = 0;
        while *(p.add(n) as *const u8) != 0 {
            n += 1;
        }
        n
    }
}

/// Byte-oriented writer to a WA CRT `FILE*` stream.
///
/// Caller borrows the stream for the lifetime of the sink. All `write_*`
/// methods go through a single byte path so recoding — when enabled — is
/// applied uniformly regardless of whether a string, literal, or number
/// is being emitted.
pub struct LogOutput {
    stream: *mut c_void,
    recode: bool,
}

impl LogOutput {
    #[inline]
    pub unsafe fn new(stream: *mut c_void) -> Self {
        Self {
            stream,
            recode: unsafe { codepage_recode_on() },
        }
    }

    /// Raw write: apply codepage LUT if enabled, then `fputc` each byte.
    pub unsafe fn write_bytes(&mut self, bytes: &[u8]) {
        unsafe {
            if self.recode {
                let lut = codepage_lut();
                for &b in bytes {
                    putc(*lut.add(b as usize + 0x100), self.stream);
                }
            } else {
                for &b in bytes {
                    putc(b, self.stream);
                }
            }
        }
    }

    #[inline]
    pub unsafe fn write_byte(&mut self, b: u8) {
        unsafe { self.write_bytes(&[b]) }
    }

    /// Bypass codepage recoding. Use for literal bytes that the original
    /// always wrote via direct `fprintf`/`fputc` (e.g. the `•••` banner),
    /// which never passed through the scratch-buffer LUT pass.
    pub unsafe fn write_raw_bytes(&mut self, bytes: &[u8]) {
        unsafe {
            for &b in bytes {
                putc(b, self.stream);
            }
        }
    }

    /// ASCII-only literal. Recoding is still applied (identity for ASCII).
    #[inline]
    pub unsafe fn write_str(&mut self, s: &str) {
        unsafe { self.write_bytes(s.as_bytes()) }
    }

    /// Null-terminated C string from WA memory.
    pub unsafe fn write_cstr(&mut self, p: *const c_char) {
        unsafe {
            if p.is_null() {
                return;
            }
            let n = strlen(p);
            self.write_bytes(core::slice::from_raw_parts(p as *const u8, n));
        }
    }

    /// `%u` — unsigned decimal.
    pub unsafe fn write_u32(&mut self, n: u32) {
        let mut buf = [0u8; 10];
        let mut i = buf.len();
        let mut v = n;
        if v == 0 {
            i -= 1;
            buf[i] = b'0';
        } else {
            while v > 0 {
                i -= 1;
                buf[i] = b'0' + (v % 10) as u8;
                v /= 10;
            }
        }
        unsafe { self.write_bytes(&buf[i..]) };
    }

    /// `%02u` — zero-padded, 2-digit.
    pub unsafe fn write_u32_02(&mut self, n: u32) {
        let b = [b'0' + ((n / 10) % 10) as u8, b'0' + (n % 10) as u8];
        unsafe { self.write_bytes(&b) };
    }

    /// `HH:MM:SS.CC` at 50 fps. Matches `DDGameWrapper__WriteHeadlessLog`
    /// (0x0053F0A0). Caller guarantees `frame >= 0`.
    pub unsafe fn write_timestamp_frames(&mut self, frame: u32) {
        let (h, rem) = (frame / 180_000, frame % 180_000);
        let (m, rem) = (rem / 3_000, rem % 3_000);
        let (s, sub) = (rem / 50, rem % 50);
        let cs = (sub * 100) / 50;
        unsafe {
            self.write_u32_02(h);
            self.write_byte(b':');
            self.write_u32_02(m);
            self.write_byte(b':');
            self.write_u32_02(s);
            self.write_byte(b'.');
            self.write_u32_02(cs);
        }
    }

    /// Emit `n` ASCII spaces. No-op when `n <= 0`.
    pub unsafe fn write_spaces(&mut self, n: i32) {
        if n <= 0 {
            return;
        }
        const CHUNK: [u8; 32] = [b' '; 32];
        let mut left = n as usize;
        while left > 0 {
            let k = left.min(CHUNK.len());
            unsafe { self.write_bytes(&CHUNK[..k]) };
            left -= k;
        }
    }
}
