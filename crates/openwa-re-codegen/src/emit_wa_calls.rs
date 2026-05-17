//! Generate typed `unsafe fn` wrappers for calling WA.exe functions from Rust.
//!
//! Two code paths:
//!
//! - **default-storage path** (custom_storage = false + one of the four base conventions):
//!   body is a pointer transmute of `rb(va::Name)` to a typed
//!   `unsafe extern "X" fn(...)`. No asm — the Rust ABI side handles
//!   register/stack allocation correctly because storage matches the
//!   convention's default.
//!
//! - **custom-storage path** (custom_storage = true, a.k.a. usercall): a thin public
//!   wrapper computes `rb(va::Name)` and forwards into a paired
//!   `#[unsafe(naked)]` shim whose body is a `core::arch::naked_asm!`
//!   trampoline. The shim's cdecl signature has `target: u32` as the first
//!   arg; the asm pulls register-stored params out of its incoming frame
//!   into their declared registers, pushes stack-stored params in reverse
//!   offset order, calls the WA function indirectly, and (for `__cdecl`
//!   callees) cleans the pushed args before returning. EAX carries the
//!   return value per cdecl. `naked_asm!` sidesteps the register-allocator
//!   pressure of plain `asm!` and lets us bind ESI/EDI/EBX without issue.
//!
//! Functions get skipped without emission when:
//!   - calling convention is missing or unrecognised
//!   - return type is missing (no `[function.signature]`)
//!   - any param type or the return type can't be resolved (Unresolved →
//!     missing `rust_path` on a struct/enum/typedef)
//!   - the TOML name doesn't parse as `Class__member` (free functions
//!     without a class prefix land in the `free` submodule instead)
//!   - **custom-storage path only:** any param has no `storage` field despite
//!     `custom_storage = true` (TOML bug)
//!   - **custom-storage path only:** any param uses a register pair (`EDX:EAX`) or an
//!     unknown register — deferred until we hit a concrete need
//!
//! Output is grouped by class:
//!
//! ```ignore
//! pub mod wa_calls {
//!     pub mod GameRuntime {
//!         #[inline]
//!         pub unsafe fn StepFrame(
//!             this: *mut openwa_game::engine::runtime::GameRuntime,
//!             frame: i32,
//!         ) -> i32 { /* … */ }
//!     }
//!     pub mod free { /* unclassified */ }
//! }
//! ```

use openwa_re_data::model::{Function, Param, Signature};
use openwa_re_data::toml_io::Catalog;
use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;

use crate::storage::{Reg, Storage};
use crate::type_resolver::parse_type_ref;

/// Per-run statistics. `skipped_*` counts are non-fatal — they exist so the
/// build script can print a one-line summary and so unit tests can assert
/// expected coverage.
#[derive(Debug, Default)]
pub struct EmitStats {
    pub functions_emitted_default_storage: usize,
    pub functions_emitted_custom_storage: usize,
    pub skipped_no_convention: usize,
    pub skipped_unknown_convention: usize,
    pub skipped_no_return_type: usize,
    pub skipped_unresolved_type: usize,
    pub skipped_invalid_member_ident: usize,
    /// `Class__member` collision (rare; e.g. two TOML entries with the same
    /// Class+member at different VAs). Lowest-VA wins.
    pub skipped_duplicate_member: usize,
    /// custom-storage path: `custom_storage = true` but some param has no `storage` field.
    /// TOML bug; surface in the build log so it gets fixed.
    pub skipped_usercall_missing_storage: usize,
    /// custom-storage path: some storage spec is a register pair (`EDX:EAX`) or a register
    /// not yet supported by the asm template. Deferred until a real need.
    pub skipped_usercall_register_pair: usize,
    /// custom-storage path: bad storage string the parser rejected.
    pub skipped_usercall_invalid_storage: usize,
    /// custom-storage path: param uses EBP — the frame pointer, which we don't bind.
    /// (ESI/EDI/EBX work fine with `naked_asm!`.)
    pub skipped_usercall_reserved_register: usize,
}

impl EmitStats {
    pub fn functions_emitted(&self) -> usize {
        self.functions_emitted_default_storage + self.functions_emitted_custom_storage
    }
}

pub fn generate(cat: &Catalog) -> (String, EmitStats) {
    let mut out = String::with_capacity(128 * cat.functions.len());
    let mut stats = EmitStats::default();

    write_header(&mut out);

    // Group functions by class (or `free` for unclassified).
    //
    // BTreeMap → deterministic alphabetical class order. Within a class, sort
    // by VA so the file diffs cleanly when a function moves but its class
    // doesn't.
    let mut by_class: BTreeMap<String, Vec<&Function>> = BTreeMap::new();
    let mut funcs: Vec<&Function> = cat.functions.values().map(|e| &e.value).collect();
    funcs.sort_by_key(|f| f.va);
    for f in funcs {
        let class = class_prefix(&f.name).unwrap_or("free").to_string();
        by_class.entry(class).or_default().push(f);
    }

    out.push_str("pub mod wa_calls {\n");
    for (class, members) in &by_class {
        write_class_module(&mut out, cat, class, members, &mut stats);
    }
    out.push_str("} // mod wa_calls\n");

    (out, stats)
}

fn write_header(out: &mut String) {
    // No inner attributes — `include!` doesn't allow them; the enclosing
    // `src/generated/mod.rs` carries the `#![allow]`s.
    out.push_str("// GENERATED by openwa-re-codegen. DO NOT EDIT.\n");
    out.push_str(
        "// Source: re/**/*.toml + rust_path mappings on struct/enum/typedef entries.\n\n",
    );
}

fn write_class_module(
    out: &mut String,
    cat: &Catalog,
    class: &str,
    members: &[&Function],
    stats: &mut EmitStats,
) {
    // Buffer this class's content so we can skip the whole module if every
    // member gets filtered out (otherwise we emit `pub mod Foo {}` with
    // nothing in it — harmless but noisy in the generated file).
    let mut body = String::new();
    let mut seen_members: HashSet<&str> = HashSet::new();
    let mut emitted_any = false;

    for f in members {
        let member = member_part(&f.name).unwrap_or(f.name.as_str());

        // Filters that apply to both paths — order matters for stat attribution.
        let Some(cc_kw) = base_calling_convention(f) else {
            if f.calling_convention.is_none() {
                stats.skipped_no_convention += 1;
            } else {
                stats.skipped_unknown_convention += 1;
            }
            continue;
        };
        let Some(signature) = f.signature.as_ref() else {
            stats.skipped_no_return_type += 1;
            continue;
        };
        if !is_valid_rust_ident(member) {
            stats.skipped_invalid_member_ident += 1;
            continue;
        }
        if !seen_members.insert(member) {
            stats.skipped_duplicate_member += 1;
            continue;
        }

        // Resolve every type. Bail to skipped_unresolved_type on first miss
        // — recording exactly which type is unresolved is the job of a
        // separate audit log (TODO when the skipped count gets noisy).
        let Some(resolved_params) = resolve_params(cat, &f.param) else {
            stats.skipped_unresolved_type += 1;
            continue;
        };
        let Some(resolved_ret) = resolve_return(cat, signature) else {
            stats.skipped_unresolved_type += 1;
            continue;
        };

        if f.custom_storage {
            // custom-storage path return type must round-trip `u32 as ReturnType`. Skip
            // exotic returns (bool, newtypes, > 4 bytes) until we have a
            // real case demanding more.
            if let Some(r) = &resolved_ret
                && !return_type_u32_castable(r)
            {
                stats.skipped_usercall_invalid_storage += 1;
                continue;
            }
            let parsed = match parse_usercall_storage(&resolved_params, &f.param, signature) {
                Ok(p) => p,
                Err(skip) => {
                    match skip {
                        UsercallSkip::MissingStorage => stats.skipped_usercall_missing_storage += 1,
                        UsercallSkip::RegisterPair => stats.skipped_usercall_register_pair += 1,
                        UsercallSkip::InvalidStorage => stats.skipped_usercall_invalid_storage += 1,
                        UsercallSkip::ReservedRegister => {
                            stats.skipped_usercall_reserved_register += 1
                        }
                    }
                    continue;
                }
            };
            write_wrapper_custom_storage(&mut body, f, member, cc_kw, &parsed, &resolved_ret);
            stats.functions_emitted_custom_storage += 1;
        } else {
            write_wrapper_default_storage(
                &mut body,
                f,
                member,
                cc_kw,
                &resolved_params,
                &resolved_ret,
            );
            stats.functions_emitted_default_storage += 1;
        }
        emitted_any = true;
    }

    if !emitted_any {
        return;
    }
    writeln!(out, "    #[allow(non_snake_case)]").unwrap();
    writeln!(out, "    pub mod {class} {{").unwrap();
    out.push_str(&body);
    writeln!(out, "    }} // mod {class}").unwrap();
}

/// Each default-storage path wrapper:
///
/// ```ignore
/// #[inline]
/// pub unsafe fn Member(arg1: T1, arg2: T2) -> R {
///     let f: unsafe extern "<conv>" fn(T1, T2) -> R =
///         core::mem::transmute(
///             crate::rebase::rb(crate::generated::addresses::<TomlName>),
///         );
///     f(arg1, arg2)
/// }
/// ```
///
/// `R` is omitted when the return type renders as `core::ffi::c_void` (TOML
/// `returns = "void"`) — Rust's empty return is implicit.
fn write_wrapper_default_storage(
    out: &mut String,
    f: &Function,
    member: &str,
    cc_kw: &str,
    params: &[(String, String)], // (name, rendered type)
    ret: &Option<String>,        // None ↔ void
) {
    // Pretty-print signatures only when they're long enough to warrant wrapping.
    let sig_args = params
        .iter()
        .map(|(n, t)| format!("{n}: {t}"))
        .collect::<Vec<_>>()
        .join(", ");
    let ret_suffix = match ret {
        Some(r) => format!(" -> {r}"),
        None => String::new(),
    };
    let fn_arg_types = params
        .iter()
        .map(|(_, t)| t.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let call_args = params
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(", ");

    // Rust 2024 edition: `unsafe fn` no longer makes its body implicitly
    // unsafe, so we explicitly wrap the transmute + call in `unsafe { … }`.
    writeln!(out, "        #[inline]").unwrap();
    writeln!(
        out,
        "        pub unsafe fn {member}({sig_args}){ret_suffix} {{",
    )
    .unwrap();
    writeln!(out, "            unsafe {{").unwrap();
    writeln!(
        out,
        "                let f: unsafe extern \"{cc_kw}\" fn({fn_arg_types}){ret_suffix} =",
    )
    .unwrap();
    writeln!(
        out,
        "                    core::mem::transmute(crate::rebase::rb(crate::generated::addresses::{}));",
        f.name,
    )
    .unwrap();
    writeln!(out, "                f({call_args})").unwrap();
    writeln!(out, "            }}").unwrap();
    writeln!(out, "        }}").unwrap();
}

fn resolve_params(cat: &Catalog, params: &[Param]) -> Option<Vec<(String, String)>> {
    let mut out = Vec::with_capacity(params.len());
    for (i, p) in params.iter().enumerate() {
        let ty = parse_type_ref(&p.ty, cat).render().ok()?;
        // Sanitise the param name — TOML allows raw C identifiers; we need
        // valid Rust idents that don't shadow our locals (`f`, `target`,
        // `ret_bits`, `tgt`) or any param-bits suffix we'd synthesise, and
        // aren't Rust reserved keywords (`self`, `type`, `match`, …). Fall
        // back to `arg<i>` when any of those rules fire.
        let conflicts = matches!(p.name.as_str(), "f" | "target" | "ret_bits" | "tgt");
        let name = if !is_valid_rust_ident(&p.name) || conflicts || is_rust_keyword(&p.name) {
            format!("arg{i}")
        } else {
            p.name.clone()
        };
        out.push((name, ty));
    }
    Some(out)
}

// ─── custom-storage path: usercall via core::arch::asm! ─────────────────────────────────

#[derive(Debug)]
enum UsercallSkip {
    /// `custom_storage = true` but some param has no `storage` field.
    MissingStorage,
    /// Param or return uses a register pair (`EDX:EAX`). custom-storage path doesn't
    /// emit 64-bit register-pair plumbing yet — add when a real call site
    /// needs it.
    RegisterPair,
    /// Storage string didn't parse, return_storage names a non-EAX
    /// register, or the rendered return type isn't `u32`-castable.
    InvalidStorage,
    /// Param uses ESI/EDI/EBX/EBP — registers that `core::arch::asm!` can't
    /// bind directly on x86 (see `EmitStats::skipped_usercall_reserved_register`).
    ReservedRegister,
}

#[derive(Debug)]
enum UsercallStorage {
    Reg(Reg),
    Stack(u32), // byte offset, ascending from [esp+0x4]
}

#[derive(Debug)]
struct UsercallParam {
    name: String,
    ty: String,
    storage: UsercallStorage,
}

#[derive(Debug)]
struct ParsedUsercall {
    params: Vec<UsercallParam>,
}

fn parse_usercall_storage(
    resolved: &[(String, String)],
    raw: &[Param],
    sig: &Signature,
) -> Result<ParsedUsercall, UsercallSkip> {
    // Return storage: implicit EAX is fine. EDX:EAX (pair) skipped; any
    // other override is bug-prone and skipped.
    if let Some(rs) = &sig.return_storage {
        let parsed_ret = crate::storage::parse(rs).map_err(|_| UsercallSkip::InvalidStorage)?;
        match parsed_ret {
            Storage::Register(Reg::Eax) => {}
            Storage::Pair(_, _) => return Err(UsercallSkip::RegisterPair),
            _ => return Err(UsercallSkip::InvalidStorage),
        }
    }

    let mut params = Vec::with_capacity(resolved.len());
    for ((name, ty), p) in resolved.iter().zip(raw) {
        let s = p.storage.as_deref().ok_or(UsercallSkip::MissingStorage)?;
        let parsed = crate::storage::parse(s).map_err(|_| UsercallSkip::InvalidStorage)?;
        let storage = match parsed {
            Storage::Register(r) => {
                // EBP is the frame pointer; binding it inside our naked
                // shim would require frame reconstruction. Defer.
                if r == Reg::Ebp {
                    return Err(UsercallSkip::ReservedRegister);
                }
                UsercallStorage::Reg(r)
            }
            Storage::Stack { offset, .. } => UsercallStorage::Stack(offset),
            Storage::Pair(_, _) => return Err(UsercallSkip::RegisterPair),
        };
        params.push(UsercallParam {
            name: name.clone(),
            ty: ty.clone(),
            storage,
        });
    }
    Ok(ParsedUsercall { params })
}

/// True when `u32 as <ty>` compiles. Conservative whitelist — easier to grow
/// than to debug a generated build failure.
fn return_type_u32_castable(rendered: &str) -> bool {
    rendered.starts_with("*mut ")
        || rendered.starts_with("*const ")
        || matches!(
            rendered,
            "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "usize" | "isize"
        )
}

/// Each custom-storage path emission is two paired items:
///
/// ```ignore
/// #[inline]
/// pub unsafe fn ShouldContinueFrameLoop(
///     wrapper: *mut core::ffi::c_void,
///     elapsed_lo: u32,
///     elapsed_hi: u32,
/// ) -> u32 {
///     unsafe {
///         _shim_GameRuntime__ShouldContinueFrameLoop(
///             crate::rebase::rb(crate::generated::addresses::GameRuntime__ShouldContinueFrameLoop),
///             wrapper, elapsed_lo, elapsed_hi,
///         )
///     }
/// }
///
/// #[unsafe(naked)]
/// unsafe extern "cdecl" fn _shim_GameRuntime__ShouldContinueFrameLoop(
///     _target: u32, _wrapper: *mut core::ffi::c_void,
///     _elapsed_lo: u32, _elapsed_hi: u32,
/// ) -> u32 {
///     core::arch::naked_asm!(
///         "mov ecx, [esp+4]",          // ECX = target
///         "mov eax, [esp+8]",          // EAX = wrapper (usercall reg)
///         "push DWORD PTR [esp+16]",   // push elapsed_hi (was at +16)
///         "push DWORD PTR [esp+16]",   // push elapsed_lo (was at +12, +4 from prior push)
///         "call ecx",
///         // __stdcall callee popped 8 bytes — no caller cleanup
///         "ret",
///     )
/// }
/// ```
///
/// The shim's incoming cdecl frame is [retaddr, target, p0, p1, …]. We read
/// each param from its position, accounting for prior callee-save pushes
/// and stack-arg pushes that shift `esp`. ESI/EDI/EBX are preserved via
/// explicit `push`/`pop` when they're used as register-input slots or as
/// the target-scratch register.
///
/// Assumes the wrapper return type is u32-castable (checked upstream).
fn write_wrapper_custom_storage(
    out: &mut String,
    f: &Function,
    member: &str,
    cc_kw: &str,
    parsed: &ParsedUsercall,
    ret: &Option<String>,
) {
    let sig_args = parsed
        .params
        .iter()
        .map(|p| format!("{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let shim_args = parsed
        .params
        .iter()
        .map(|p| format!("_{}: {}", p.name, p.ty))
        .collect::<Vec<_>>()
        .join(", ");
    let call_through = parsed
        .params
        .iter()
        .map(|p| p.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let ret_suffix = match ret {
        Some(r) => format!(" -> {r}"),
        None => String::new(),
    };
    let shim_name = format!("_shim_{}", f.name);

    // ── Public wrapper: compute target via rb(va), forward to the shim ──
    writeln!(out, "        #[inline]").unwrap();
    writeln!(
        out,
        "        pub unsafe fn {member}({sig_args}){ret_suffix} {{",
    )
    .unwrap();
    writeln!(out, "            unsafe {{").unwrap();
    writeln!(out, "                {shim_name}(").unwrap();
    writeln!(
        out,
        "                    crate::rebase::rb(crate::generated::addresses::{}),",
        f.name,
    )
    .unwrap();
    if !parsed.params.is_empty() {
        writeln!(out, "                    {call_through},").unwrap();
    }
    writeln!(out, "                )").unwrap();
    writeln!(out, "            }}").unwrap();
    writeln!(out, "        }}").unwrap();

    // ── Naked shim ──────────────────────────────────────────────────────
    writeln!(out).unwrap();
    writeln!(out, "        #[unsafe(naked)]").unwrap();
    let shim_sig_args = if parsed.params.is_empty() {
        String::new()
    } else {
        format!(", {shim_args}")
    };
    writeln!(
        out,
        "        unsafe extern \"cdecl\" fn {shim_name}(_target: u32{shim_sig_args}){ret_suffix} {{",
    )
    .unwrap();
    writeln!(out, "            core::arch::naked_asm!(").unwrap();

    // Pick which register to load `target` into. Prefer caller-saved
    // (EAX/ECX/EDX) that isn't a usercall register input; fall back to
    // EBX (callee-saved → preserved below).
    let used_input_regs: HashSet<Reg> = parsed
        .params
        .iter()
        .filter_map(|p| match p.storage {
            UsercallStorage::Reg(r) => Some(r),
            _ => None,
        })
        .collect();
    let target_reg = [Reg::Ecx, Reg::Edx, Reg::Eax]
        .into_iter()
        .find(|r| !used_input_regs.contains(r))
        .unwrap_or(Reg::Ebx);

    // Callee-saved regs we'll touch — these need explicit `push`/`pop`
    // around the body to honour cdecl's preservation contract.
    let preserved: Vec<Reg> = [Reg::Ebx, Reg::Esi, Reg::Edi]
        .into_iter()
        .filter(|r| used_input_regs.contains(r) || *r == target_reg)
        .collect();
    let saved_bytes = preserved.len() as u32 * 4;

    // Step 1: save callee-saved regs.
    for r in &preserved {
        writeln!(out, "                \"push {}\",", r.asm_name()).unwrap();
    }

    // Step 2: load `target` into target_reg. After the saves, `target` sits
    // at [esp + 4 + saved_bytes] in the shim's incoming frame.
    let target_offset_now = 4 + saved_bytes;
    writeln!(
        out,
        "                \"mov {}, [esp+{}]\",",
        target_reg.asm_name(),
        target_offset_now
    )
    .unwrap();

    // Step 3: load each register-stored usercall param from its incoming
    // stack slot into the declared register. The cdecl shim layout is:
    //   [esp+0]=ret_addr, [esp+4]=target, [esp+8]=p0, [esp+12]=p1, …
    // so param at index i lives at [esp + 8 + i*4]; shifted by saved_bytes.
    for (i, p) in parsed.params.iter().enumerate() {
        if let UsercallStorage::Reg(r) = p.storage {
            let off = 8 + (i as u32) * 4 + saved_bytes;
            writeln!(
                out,
                "                \"mov {}, [esp+{}]\",",
                r.asm_name(),
                off
            )
            .unwrap();
        }
    }

    // Step 4: push stack-stored args in REVERSE offset order so the lowest
    // declared offset ends up at the lowest stack address after pushes
    // (matching what the WA callee reads at [esp+4] on entry).
    let mut stack_params: Vec<(usize, &UsercallParam)> = parsed
        .params
        .iter()
        .enumerate()
        .filter(|(_, p)| matches!(p.storage, UsercallStorage::Stack(_)))
        .collect();
    stack_params.sort_by_key(|(_, p)| match p.storage {
        UsercallStorage::Stack(o) => o,
        _ => unreachable!(),
    });
    let mut pushed = 0u32;
    for (i, _) in stack_params.iter().rev() {
        let off = 8 + (*i as u32) * 4 + saved_bytes + pushed * 4;
        writeln!(out, "                \"push DWORD PTR [esp+{}]\",", off).unwrap();
        pushed += 1;
    }

    // Step 5: indirect call through target_reg.
    writeln!(out, "                \"call {}\",", target_reg.asm_name()).unwrap();

    // Step 6: cdecl callee leaves stack args in place; we clean them up.
    // Other conventions (stdcall/thiscall/fastcall) clean via `ret imm16`.
    if cc_kw == "cdecl" && pushed > 0 {
        writeln!(out, "                \"add esp, {}\",", pushed * 4).unwrap();
    }

    // Step 7: restore callee-saved regs in reverse save order.
    for r in preserved.iter().rev() {
        writeln!(out, "                \"pop {}\",", r.asm_name()).unwrap();
    }

    // Step 8: cdecl shim — caller cleans our incoming args. EAX already
    // holds the return value per cdecl + usercall convention.
    writeln!(out, "                \"ret\",").unwrap();
    writeln!(out, "            )").unwrap();
    writeln!(out, "        }}").unwrap();

    let _ = ret;
}

/// Names that can't be used as a free-function parameter in Rust. Covers
/// strict keywords + the handful of reserved words that show up in C
/// signatures (`type`, `self`, `match`, …). Not exhaustive of every Rust
/// keyword — only those that would actually appear in TOML param names.
fn is_rust_keyword(s: &str) -> bool {
    matches!(
        s,
        "as" | "async"
            | "await"
            | "box"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "do"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "priv"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "typeof"
            | "union"
            | "unsafe"
            | "unsized"
            | "use"
            | "virtual"
            | "where"
            | "while"
            | "yield"
    )
}

/// Returns `None` if the return type doesn't resolve. Returns `Some(None)` for
/// `void` (so the wrapper omits the `-> R` clause).
fn resolve_return(cat: &Catalog, sig: &Signature) -> Option<Option<String>> {
    let rendered = parse_type_ref(&sig.returns, cat).render().ok()?;
    Some(if rendered == "core::ffi::c_void" {
        None
    } else {
        Some(rendered)
    })
}

/// Map TOML `calling_convention` string to the Rust ABI keyword used inside
/// `extern "X" fn`. Returns `None` for missing or unrecognised values, and
/// for any function with `custom_storage = true` (caller filters earlier).
fn base_calling_convention(f: &Function) -> Option<&'static str> {
    match f.calling_convention.as_deref()? {
        "__stdcall" => Some("stdcall"),
        "__cdecl" => Some("cdecl"),
        "__thiscall" => Some("thiscall"),
        "__fastcall" => Some("fastcall"),
        _ => None,
    }
}

fn class_prefix(name: &str) -> Option<&str> {
    let idx = name.find("__")?;
    if idx == 0 {
        return None;
    }
    let prefix = &name[..idx];
    if prefix
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_')
    {
        Some(prefix)
    } else {
        None
    }
}

fn member_part(name: &str) -> Option<&str> {
    let idx = name.find("__")?;
    Some(&name[idx + 2..])
}

fn is_valid_rust_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use openwa_re_data::model::{Function, Param, Signature, Struct};
    use openwa_re_data::toml_io::{Catalog, OwnedEntry};
    use std::path::PathBuf;

    fn fn_with(
        va: u32,
        name: &str,
        cc: Option<&str>,
        custom: bool,
        ret: Option<&str>,
        params: Vec<(&str, &str)>,
    ) -> Function {
        Function {
            va,
            name: name.into(),
            calling_convention: cc.map(str::to_string),
            plate_comment: None,
            no_return: false,
            custom_storage: custom,
            signature: ret.map(|r| Signature {
                returns: r.into(),
                return_storage: None,
            }),
            param: params
                .into_iter()
                .map(|(n, t)| Param {
                    name: n.into(),
                    ty: t.into(),
                    storage: None,
                })
                .collect(),
            local: vec![],
            comment: vec![],
        }
    }

    fn cat_with(fns: Vec<Function>) -> Catalog {
        let mut c = Catalog::default();
        for f in fns {
            c.functions.insert(
                f.va,
                OwnedEntry {
                    value: f,
                    source: PathBuf::from("<test>"),
                },
            );
        }
        c
    }

    fn with_struct(cat: &mut Catalog, name: &str, rust_path: &str) {
        cat.structs.insert(
            name.to_string(),
            OwnedEntry {
                value: Struct {
                    name: name.to_string(),
                    namespace: None,
                    size: 0,
                    plate_comment: None,
                    rust_path: Some(rust_path.to_string()),
                    field: vec![],
                },
                source: PathBuf::from("<test>"),
            },
        );
    }

    #[test]
    fn emits_thiscall_wrapper_with_resolved_pointer() {
        let mut cat = cat_with(vec![fn_with(
            0x00529F30,
            "GameRuntime__StepFrame",
            Some("__thiscall"),
            false,
            Some("int"),
            vec![("this", "GameRuntime *"), ("frame", "int")],
        )]);
        with_struct(
            &mut cat,
            "GameRuntime",
            "openwa_game::engine::runtime::GameRuntime",
        );

        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted(), 1);
        assert!(out.contains("pub mod GameRuntime {"));
        assert!(out.contains("pub unsafe fn StepFrame("));
        assert!(out.contains("this: *mut openwa_game::engine::runtime::GameRuntime"));
        assert!(out.contains("frame: i32"));
        assert!(out.contains("-> i32"));
        assert!(out.contains("unsafe extern \"thiscall\" fn(*mut openwa_game::engine::runtime::GameRuntime, i32) -> i32"));
        assert!(out.contains("crate::generated::addresses::GameRuntime__StepFrame"));
        assert!(out.contains("f(this, frame)"));
    }

    #[test]
    fn omits_return_type_when_void() {
        let mut cat = cat_with(vec![fn_with(
            0x00500000,
            "GameRuntime__ResetFrameState",
            Some("__cdecl"),
            false,
            Some("void"),
            vec![],
        )]);
        with_struct(
            &mut cat,
            "GameRuntime",
            "openwa_game::engine::runtime::GameRuntime",
        );
        let (out, _) = generate(&cat);
        // Signature should NOT have `-> ()` — empty return for clarity.
        assert!(out.contains("pub unsafe fn ResetFrameState()"));
        assert!(!out.contains("-> ()"));
        assert!(out.contains("unsafe extern \"cdecl\" fn()"));
    }

    #[test]
    fn usercall_without_storage_strings_is_skipped() {
        // `custom_storage = true` but every param has `storage: None` —
        // a TOML bug. custom-storage path refuses to guess.
        let cat = cat_with(vec![fn_with(
            0x00529F30,
            "GameRuntime__StepFrame",
            Some("__thiscall"),
            true, // custom_storage
            Some("int"),
            vec![("this", "int")],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.skipped_usercall_missing_storage, 1);
        assert_eq!(stats.functions_emitted(), 0);
        assert!(!out.contains("StepFrame"));
    }

    fn fn_with_storage(
        va: u32,
        name: &str,
        cc: Option<&str>,
        ret: Option<&str>,
        params: Vec<(&str, &str, &str)>, // (name, type, storage)
    ) -> Function {
        Function {
            va,
            name: name.into(),
            calling_convention: cc.map(str::to_string),
            plate_comment: None,
            no_return: false,
            custom_storage: true,
            signature: ret.map(|r| Signature {
                returns: r.into(),
                return_storage: None,
            }),
            param: params
                .into_iter()
                .map(|(n, t, s)| Param {
                    name: n.into(),
                    ty: t.into(),
                    storage: Some(s.into()),
                })
                .collect(),
            local: vec![],
            comment: vec![],
        }
    }

    #[test]
    fn emits_path_b_for_eax_register_param_and_stack_params() {
        // `GameRuntime__ShouldContinueFrameLoop` — the canonical custom-storage path target.
        // EAX = wrapper, stack:0x4 = elapsed_lo, stack:0x8 = elapsed_hi.
        // __stdcall (callee cleans 8 bytes), returns uint via EAX.
        let cat = cat_with(vec![fn_with_storage(
            0x0052A840,
            "GameRuntime__ShouldContinueFrameLoop",
            Some("__stdcall"),
            Some("uint"),
            vec![
                ("wrapper", "void *", "EAX"),
                ("elapsed_lo", "uint", "stack:0x4"),
                ("elapsed_hi", "uint", "stack:0x8"),
            ],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted_custom_storage, 1);
        assert_eq!(stats.functions_emitted_default_storage, 0);

        // Public wrapper forwards to the shim.
        assert!(out.contains("pub unsafe fn ShouldContinueFrameLoop("));
        assert!(out.contains("wrapper: *mut core::ffi::c_void"));
        assert!(out.contains("-> u32"));
        assert!(out.contains("_shim_GameRuntime__ShouldContinueFrameLoop("));
        assert!(out.contains(
            "crate::rebase::rb(crate::generated::addresses::GameRuntime__ShouldContinueFrameLoop)"
        ));

        // Naked shim contains the asm trampoline.
        assert!(out.contains("#[unsafe(naked)]"));
        assert!(
            out.contains("unsafe extern \"cdecl\" fn _shim_GameRuntime__ShouldContinueFrameLoop(")
        );
        assert!(out.contains("core::arch::naked_asm!"));

        // Asm body — target is loaded into a non-input reg; wrapper goes into EAX.
        assert!(out.contains("\"mov ecx, [esp+4]\","));
        assert!(out.contains("\"mov eax, [esp+8]\","));

        // Push order: highest offset first. Two pushes mean the second
        // reads from [esp+16] as well (the +4 shift cancels with the
        // original +12 → +16 ↔ +16 after the first push).
        assert!(out.contains("\"push DWORD PTR [esp+16]\","));

        // Indirect call through the target register.
        assert!(out.contains("\"call ecx\","));

        // __stdcall callee — no caller cleanup.
        assert!(!out.contains("\"add esp"));

        // Final return.
        assert!(out.contains("\"ret\","));
    }

    #[test]
    fn path_b_cdecl_emits_caller_stack_cleanup() {
        let cat = cat_with(vec![fn_with_storage(
            0x00400000,
            "Foo__bar",
            Some("__cdecl"),
            Some("void"),
            vec![
                ("this", "void *", "EAX"),
                ("a", "uint", "stack:0x4"),
                ("b", "uint", "stack:0x8"),
            ],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted_custom_storage, 1);
        assert!(out.contains("\"add esp, 8\","));
    }

    #[test]
    fn path_b_skips_register_pair_storage() {
        let cat = cat_with(vec![fn_with_storage(
            0x00400000,
            "Foo__bar",
            Some("__stdcall"),
            Some("void"),
            vec![("big_val", "ulonglong", "EDX:EAX")],
        )]);
        let (_, stats) = generate(&cat);
        assert_eq!(stats.skipped_usercall_register_pair, 1);
        assert_eq!(stats.functions_emitted(), 0);
    }

    #[test]
    fn path_b_skips_non_eax_return_storage() {
        // Synthetic: a usercall claiming it returns in EDX. We don't
        // support that yet — skip cleanly.
        let cat = cat_with(vec![Function {
            va: 0x00400000,
            name: "Foo__bar".into(),
            calling_convention: Some("__stdcall".into()),
            plate_comment: None,
            no_return: false,
            custom_storage: true,
            signature: Some(Signature {
                returns: "uint".into(),
                return_storage: Some("EDX".into()),
            }),
            param: vec![Param {
                name: "x".into(),
                ty: "uint".into(),
                storage: Some("EAX".into()),
            }],
            local: vec![],
            comment: vec![],
        }]);
        let (_, stats) = generate(&cat);
        assert_eq!(stats.skipped_usercall_invalid_storage, 1);
    }

    #[test]
    fn path_b_void_return_with_ecx_param() {
        // ECX is used as input, so target picks EDX instead.
        let cat = cat_with(vec![fn_with_storage(
            0x00400000,
            "Foo__bar",
            Some("__stdcall"),
            Some("void"),
            vec![("this", "void *", "ECX")],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted_custom_storage, 1);
        // Target reg = EDX (since ECX is the input).
        assert!(out.contains("\"mov edx, [esp+4]\","));
        assert!(out.contains("\"mov ecx, [esp+8]\","));
        assert!(out.contains("\"call edx\","));
    }

    #[test]
    fn path_b_supports_esi_param_via_naked_asm() {
        // ESI is callee-saved per cdecl; the shim must push/pop it.
        let cat = cat_with(vec![fn_with_storage(
            0x00400000,
            "Foo__bar",
            Some("__stdcall"),
            Some("void"),
            vec![("this", "void *", "ESI")],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted_custom_storage, 1);
        // Preserve + restore ESI.
        let push_esi = out.find("\"push esi\",").expect("push esi");
        let pop_esi = out.find("\"pop esi\",").expect("pop esi");
        assert!(push_esi < pop_esi, "push must come before pop");
        // After `push esi`, target sits at [esp+8] and the ESI input at [esp+12].
        assert!(out.contains("\"mov esi, [esp+12]\","));
    }

    #[test]
    fn skips_when_param_type_unresolved() {
        let cat = cat_with(vec![fn_with(
            0x00500000,
            "Foo__bar",
            Some("__stdcall"),
            false,
            Some("int"),
            vec![("x", "BaseEntity *")], // no rust_path → unresolved
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.skipped_unresolved_type, 1);
        assert_eq!(stats.functions_emitted(), 0);
        assert!(!out.contains("Foo__bar"));
    }

    #[test]
    fn skips_when_no_signature_block() {
        let cat = cat_with(vec![fn_with(
            0x00500000,
            "Foo__bar",
            Some("__stdcall"),
            false,
            None, // no signature
            vec![],
        )]);
        let (_, stats) = generate(&cat);
        assert_eq!(stats.skipped_no_return_type, 1);
    }

    #[test]
    fn sanitises_param_name_when_invalid_or_shadowing() {
        let cat = cat_with(vec![fn_with(
            0x00500000,
            "Foo__bar",
            Some("__cdecl"),
            false,
            Some("int"),
            vec![
                ("0bad", "int"), // invalid → arg0
                ("f", "int"),    // shadows local `f` → arg1
            ],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted(), 1);
        assert!(out.contains("arg0: i32"));
        assert!(out.contains("arg1: i32"));
        assert!(out.contains("f(arg0, arg1)"));
    }

    #[test]
    fn unclassified_function_goes_in_free_module() {
        let cat = cat_with(vec![fn_with(
            0x0053F320,
            "AdvanceGameRng",
            Some("__fastcall"),
            false,
            Some("uint"),
            vec![("state", "uint")],
        )]);
        let (out, stats) = generate(&cat);
        assert_eq!(stats.functions_emitted(), 1);
        assert!(out.contains("pub mod free {"));
        assert!(out.contains("pub unsafe fn AdvanceGameRng("));
    }

    #[test]
    fn empty_class_module_is_not_emitted() {
        // Every member is usercall → no wrappers → don't emit `pub mod Foo {}`.
        let cat = cat_with(vec![fn_with(
            0x00500000,
            "Foo__bar",
            Some("__thiscall"),
            true,
            Some("void"),
            vec![("this", "int")],
        )]);
        let (out, _) = generate(&cat);
        assert!(!out.contains("pub mod Foo"));
    }
}
