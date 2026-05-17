//! Parser for `Param.storage` strings declared in `re/*.toml`.
//!
//! Grammar (mirrors `crates/openwa-re-data/src/model.rs:288-297`):
//!
//! ```text
//! storage  := register | reg_pair | stack
//! register := <reg-name>                 e.g. "ECX", "EAX", "ESI"
//! reg_pair := <reg-name> ":" <reg-name>  e.g. "EDX:EAX"
//! stack    := "stack:" <offset> [ ":" <size> ]
//! ```
//!
//! Numeric fields accept `0x`-prefixed hex or decimal. Register names are
//! case-insensitive on input; we normalise to UPPERCASE in the parsed form
//! so trampoline asm and `asm!` constraint strings can lowercase via `to_lowercase()`.

use anyhow::{Result, anyhow, bail};

/// Parsed form of a `storage = "..."` string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Storage {
    /// Single register, e.g. `ECX`.
    Register(Reg),
    /// Two registers combined (typically a 64-bit value), e.g. `EDX:EAX`.
    /// Order is preserved verbatim from the TOML; semantics (low/high)
    /// follow Ghidra's storage encoding for the given convention.
    Pair(Reg, Reg),
    /// Stack slot at byte offset `offset` from the callee's incoming `[esp]`
    /// (so the first stack param sits at `0x4`, past the return address).
    /// `size` is `None` when Ghidra derives it from the parameter type.
    Stack { offset: u32, size: Option<u32> },
}

/// An x86 GP register usable as a storage slot. Only the names that actually
/// appear in WA's RE database are listed; if a TOML adds e.g. an EBP slot we
/// surface a parse error rather than silently accept it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reg {
    Eax,
    Ebx,
    Ecx,
    Edx,
    Esi,
    Edi,
    Ebp,
}

impl Reg {
    /// Parse a register name. Case-insensitive on input; returns `None` for
    /// unrecognised names. Named `parse` (not `from_str`) to avoid colliding
    /// with `std::str::FromStr::from_str` (which would have a different
    /// `Result<_, _>` return type).
    pub fn parse(s: &str) -> Option<Reg> {
        match s.trim().to_ascii_uppercase().as_str() {
            "EAX" => Some(Reg::Eax),
            "EBX" => Some(Reg::Ebx),
            "ECX" => Some(Reg::Ecx),
            "EDX" => Some(Reg::Edx),
            "ESI" => Some(Reg::Esi),
            "EDI" => Some(Reg::Edi),
            "EBP" => Some(Reg::Ebp),
            _ => None,
        }
    }

    /// Lowercase asm-syntax name, e.g. `"ecx"`. Suitable for `push <reg>` /
    /// inline-asm constraint strings.
    pub fn asm_name(self) -> &'static str {
        match self {
            Reg::Eax => "eax",
            Reg::Ebx => "ebx",
            Reg::Ecx => "ecx",
            Reg::Edx => "edx",
            Reg::Esi => "esi",
            Reg::Edi => "edi",
            Reg::Ebp => "ebp",
        }
    }
}

pub fn parse(s: &str) -> Result<Storage> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        bail!("empty storage string");
    }

    // stack:OFFSET[:SIZE]
    if let Some(rest) = trimmed
        .strip_prefix("stack:")
        .or_else(|| trimmed.strip_prefix("STACK:"))
    {
        let mut parts = rest.split(':');
        let offset_s = parts
            .next()
            .ok_or_else(|| anyhow!("missing stack offset in {s:?}"))?;
        let offset = parse_int(offset_s)
            .ok_or_else(|| anyhow!("invalid stack offset {offset_s:?} in {s:?}"))?;
        let size = match parts.next() {
            None => None,
            Some(size_s) => Some(
                parse_int(size_s)
                    .ok_or_else(|| anyhow!("invalid stack size {size_s:?} in {s:?}"))?,
            ),
        };
        if parts.next().is_some() {
            bail!("trailing garbage after stack size in {s:?}");
        }
        return Ok(Storage::Stack { offset, size });
    }

    // REG[:REG]
    if let Some((lhs, rhs)) = trimmed.split_once(':') {
        let lo = Reg::parse(lhs)
            .ok_or_else(|| anyhow!("unrecognised register {lhs:?} in pair {s:?}"))?;
        let hi = Reg::parse(rhs)
            .ok_or_else(|| anyhow!("unrecognised register {rhs:?} in pair {s:?}"))?;
        return Ok(Storage::Pair(lo, hi));
    }

    let reg = Reg::parse(trimmed).ok_or_else(|| anyhow!("unrecognised storage spec {s:?}"))?;
    Ok(Storage::Register(reg))
}

fn parse_int(s: &str) -> Option<u32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok()
    } else {
        s.parse::<u32>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_register() {
        assert_eq!(parse("ECX").unwrap(), Storage::Register(Reg::Ecx));
        assert_eq!(parse("eax").unwrap(), Storage::Register(Reg::Eax));
        assert_eq!(parse(" ESI ").unwrap(), Storage::Register(Reg::Esi));
    }

    #[test]
    fn parses_register_pair() {
        assert_eq!(parse("EDX:EAX").unwrap(), Storage::Pair(Reg::Edx, Reg::Eax));
        assert_eq!(parse("eax:edx").unwrap(), Storage::Pair(Reg::Eax, Reg::Edx));
    }

    #[test]
    fn parses_stack_no_size() {
        assert_eq!(
            parse("stack:0x4").unwrap(),
            Storage::Stack {
                offset: 4,
                size: None
            }
        );
        assert_eq!(
            parse("stack:16").unwrap(),
            Storage::Stack {
                offset: 16,
                size: None
            }
        );
    }

    #[test]
    fn parses_stack_with_size() {
        assert_eq!(
            parse("stack:0x8:4").unwrap(),
            Storage::Stack {
                offset: 8,
                size: Some(4)
            }
        );
        assert_eq!(
            parse("stack:0x10:0x8").unwrap(),
            Storage::Stack {
                offset: 16,
                size: Some(8)
            }
        );
    }

    #[test]
    fn rejects_empty() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
    }

    #[test]
    fn rejects_unknown_register() {
        assert!(parse("R15").is_err());
        assert!(parse("EBP:R8").is_err());
    }

    #[test]
    fn rejects_malformed_stack() {
        assert!(parse("stack:").is_err());
        assert!(parse("stack:abc").is_err());
        assert!(parse("stack:0x4:0x8:0xC").is_err());
    }

    #[test]
    fn reg_asm_names_are_lowercase() {
        assert_eq!(Reg::Ecx.asm_name(), "ecx");
        assert_eq!(Reg::Edi.asm_name(), "edi");
    }
}
