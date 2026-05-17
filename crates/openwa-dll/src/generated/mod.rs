//! Build-time code generated from `crates/openwa-dll/hooks/*.toml` joined
//! against `re/**/*.toml`. Produced by `build.rs` via `openwa-re-codegen`.
//! Do not edit — change the TOML inputs and rebuild.
#![allow(non_upper_case_globals, non_snake_case, dead_code)]

include!(concat!(env!("OUT_DIR"), "/generated_trampolines.rs"));

#[cfg(test)]
mod equivalence_tests {
    //! Byte-equivalence check between hand-written `usercall_trampoline!`
    //! shims (in `crates/openwa-dll/src/hook.rs`) and the same shape emitted
    //! by `emit_trampolines`. If MinHook + WA already accept the hand-written
    //! one, a byte-identical generated one is also safe.
    //!
    //! Compares the first instructions of each function's machine code. We
    //! stop at the `ret` instruction so that randomised symbol-table layout
    //! (the `sym` address embedded in the `call rel32`) doesn't perturb the
    //! comparison — the relocation target differs between the two compiled
    //! sites, but the instruction sequence around it is identical.
    //!
    //! Scoped to `WormEntity__stop_fire_sound` for now: simplest possible
    //! usercall shape (one register arg, no stack args, plain ret).

    use super::hooks;

    /// Walk forward through the trampoline's bytes and collect a normalised
    /// digest:
    /// - skip the immediate operand of every E8 (relative call) — that's the
    ///   relocation target, which legitimately differs between the two
    ///   sites,
    /// - stop after `ret` (0xC3) or `ret imm16` (0xC2 + 2 bytes).
    fn instruction_digest(start: *const u8) -> Vec<u8> {
        let mut out = Vec::new();
        let mut p = start;
        for _ in 0..64 {
            unsafe {
                let op = *p;
                out.push(op);
                match op {
                    // CALL rel32
                    0xE8 => {
                        // skip the 4-byte rel32 displacement
                        p = p.add(5);
                        out.extend_from_slice(b"<rel32>");
                    }
                    // RET (plain)
                    0xC3 => return out,
                    // RET imm16
                    0xC2 => {
                        out.push(*p.add(1));
                        out.push(*p.add(2));
                        return out;
                    }
                    // PUSH r32 (0x50..0x57) — 1 byte
                    0x50..=0x57 | 0x58..=0x5F => p = p.add(1),
                    // ADD esp, imm8 / SUB esp, imm8 (0x83 /0 /5 + imm8) — 3 bytes
                    0x83 => {
                        out.push(*p.add(1));
                        out.push(*p.add(2));
                        p = p.add(3);
                    }
                    // PUSH imm32 (0x68 + 4 bytes) — 5 bytes
                    0x68 => {
                        for i in 1..5 {
                            out.push(*p.add(i));
                        }
                        p = p.add(5);
                    }
                    // PUSH DWORD PTR [esp+disp8] (0xFF 0x74 0x24 disp8) — 4 bytes
                    0xFF => {
                        out.push(*p.add(1));
                        out.push(*p.add(2));
                        out.push(*p.add(3));
                        p = p.add(4);
                    }
                    // Fallback: 1-byte step. Sufficient for the simple shape
                    // we test; expand when migrating more complex hooks.
                    _ => p = p.add(1),
                }
            }
        }
        out
    }

    #[test]
    fn stop_worm_sound_generated_matches_handwritten() {
        // SAFETY: we only read the function's code bytes from its entry
        // point, which is always readable. The walker terminates at `ret`
        // and is bounded to 64 instructions; no out-of-page accesses.
        let generated = instruction_digest(hooks::tramp_WormEntity__stop_fire_sound as *const u8);
        let handwritten =
            instruction_digest(crate::replacements::sound::trampoline_stop_worm_sound_for_tests());
        assert_eq!(
            generated, handwritten,
            "generated tramp_WormEntity__stop_fire_sound diverges from hand-written usercall_trampoline! shim",
        );
    }
}
