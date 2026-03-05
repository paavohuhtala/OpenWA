# Runtime Validator DLL Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a 32-bit Rust WormKit DLL that validates our `openwa-types` struct layouts and address constants against live WA.exe 3.8.1 memory.

**Architecture:** A `cdylib` crate (`openwa-validator`) compiled for `i686-pc-windows-msvc`. Loaded by WormKit's HookLib as `wkOpenWAValidator.dll`. On attach: validates addresses immediately, hooks constructors with `retour`, logs PASS/FAIL to a file.

**Tech Stack:** Rust (stable), `retour` (inline hooking), `openwa-types` (path dep), raw Win32 FFI for DllMain.

---

### Task 1: Install the i686 target

**Step 1: Add the Rust target**

Run: `rustup target add i686-pc-windows-msvc`
Expected: "installed" or "already installed"

---

### Task 2: Create the openwa-validator crate

**Files:**
- Create: `crates/openwa-validator/Cargo.toml`
- Create: `crates/openwa-validator/src/lib.rs`

**Step 1: Create Cargo.toml**

```toml
[package]
name = "openwa-validator"
version = "0.1.0"
edition = "2021"
description = "WormKit DLL that validates openwa-types against live WA.exe memory"

[lib]
crate-type = ["cdylib"]

[dependencies]
openwa-types = { path = "../openwa-types" }
retour = { version = "0.4", features = ["static-detour", "thiscall-abi"] }
```

**Step 2: Create minimal lib.rs with DllMain**

This is the DLL entry point. On `DLL_PROCESS_ATTACH`, it spawns a thread
(to avoid loader lock) that runs all validation.

```rust
#![allow(non_snake_case)]

use std::ffi::c_void;

const DLL_PROCESS_ATTACH: u32 = 1;

#[no_mangle]
unsafe extern "system" fn DllMain(
    _module: *mut c_void,
    reason: u32,
    _reserved: *mut c_void,
) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        std::thread::spawn(|| {
            if let Err(e) = run_validation() {
                let _ = log_line(&format!("[FATAL] Validation failed to run: {}", e));
            }
        });
    }
    1 // TRUE
}

fn run_validation() -> Result<(), Box<dyn std::error::Error>> {
    let _ = log_line("=== OpenWA Validator ===");
    let _ = log_line("DLL loaded successfully. Validation will follow.");
    Ok(())
}

fn log_line(msg: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("OpenWA_validation.log")?;
    writeln!(f, "{}", msg)?;
    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build --target i686-pc-windows-msvc -p openwa-validator`
Expected: Compiles successfully, produces `target/i686-pc-windows-msvc/debug/openwa_validator.dll`

**Step 4: Commit**

```
feat: scaffold openwa-validator WormKit DLL crate
```

---

### Task 3: Add address validation

**Files:**
- Modify: `crates/openwa-validator/src/lib.rs`

This runs immediately on DLL load — no hooks needed. It reads memory at our
known addresses and checks plausibility.

**Step 1: Add address validation module**

Add a `validate_addresses` function that checks:
- Vtable addresses: read first DWORD, verify it's in .text range (0x401000..0x61A000)
- Function addresses: read first bytes, verify common x86 prologue patterns
- Global variable addresses: verify they're in .data range (0x694000..0x8C4158)

```rust
use openwa_types::address::va;

struct ValidationResult {
    pass: u32,
    fail: u32,
}

impl ValidationResult {
    fn new() -> Self {
        Self { pass: 0, fail: 0 }
    }

    fn check(&mut self, name: &str, passed: bool, detail: &str) {
        if passed {
            self.pass += 1;
            let _ = log_line(&format!("[PASS] {}: {}", name, detail));
        } else {
            self.fail += 1;
            let _ = log_line(&format!("[FAIL] {}: {}", name, detail));
        }
    }

    fn summary(&self) {
        let total = self.pass + self.fail;
        let _ = log_line(&format!(
            "[INFO] {}/{} checks passed, {} failed",
            self.pass, total, self.fail
        ));
    }
}

unsafe fn read_u32(addr: u32) -> u32 {
    *(addr as *const u32)
}

unsafe fn read_u8(addr: u32) -> u8 {
    *(addr as *const u8)
}

fn is_in_text(addr: u32) -> bool {
    addr >= va::TEXT_START && addr <= va::TEXT_END
}

fn is_in_rdata(addr: u32) -> bool {
    addr >= va::RDATA_START && addr < va::DATA_START
}

fn validate_addresses(result: &mut ValidationResult) {
    unsafe {
        // --- Vtable checks: first entry should be a function pointer in .text ---
        let vtables: &[(&str, u32)] = &[
            ("CTASK_VTABLE", va::CTASK_VTABLE),
            ("CGAMETASK_VTABLE", va::CGAMETASK_VTABLE),
            ("CGAMETASK_VTABLE2", va::CGAMETASK_VTABLE2),
            ("DDGAME_WRAPPER_VTABLE", va::DDGAME_WRAPPER_VTABLE),
            ("GFX_HANDLER_VTABLE", va::GFX_HANDLER_VTABLE),
            ("DISPLAY_GFX_VTABLE", va::DISPLAY_GFX_VTABLE),
            ("PC_LANDSCAPE_VTABLE", va::PC_LANDSCAPE_VTABLE),
            ("LANDSCAPE_SHADER_VTABLE", va::LANDSCAPE_SHADER_VTABLE),
            ("DS_SOUND_VTABLE", va::DS_SOUND_VTABLE),
            ("TASK_STATE_MACHINE_VTABLE", va::TASK_STATE_MACHINE_VTABLE),
            ("OPENGL_CPU_VTABLE", va::OPENGL_CPU_VTABLE),
            ("WATER_EFFECT_VTABLE", va::WATER_EFFECT_VTABLE),
        ];

        for (name, addr) in vtables {
            // Vtable itself should be in .rdata
            let in_rdata = is_in_rdata(*addr);
            result.check(
                &format!("{} location", name),
                in_rdata,
                &format!("0x{:08X} in .rdata: {}", addr, in_rdata),
            );

            // First entry should point to .text
            let first_entry = read_u32(*addr);
            let valid = is_in_text(first_entry);
            result.check(
                &format!("{} first entry", name),
                valid,
                &format!("0x{:08X} -> 0x{:08X} (in .text: {})", addr, first_entry, valid),
            );
        }

        // --- CTask vtable content: verify known method pointers ---
        let ctask_vt_checks: &[(&str, u32, u32)] = &[
            ("CTask vt[0] init", va::CTASK_VTABLE, va::CTASK_VT0_INIT),
            ("CTask vt[1] free", va::CTASK_VTABLE + 4, va::CTASK_VT1_FREE),
            ("CTask vt[2] handleMsg", va::CTASK_VTABLE + 8, va::CTASK_VT2_HANDLE_MESSAGE),
            ("CTask vt[7] processFrame", va::CTASK_VTABLE + 0x1C, va::CTASK_VT7_PROCESS_FRAME),
        ];

        for (name, vt_addr, expected_fn) in ctask_vt_checks {
            let actual = read_u32(*vt_addr);
            result.check(
                name,
                actual == *expected_fn,
                &format!("at 0x{:08X}: expected 0x{:08X}, got 0x{:08X}", vt_addr, expected_fn, actual),
            );
        }

        // --- Function prologue checks ---
        let functions: &[(&str, u32)] = &[
            ("CTASK_CONSTRUCTOR", va::CTASK_CONSTRUCTOR),
            ("CGAMETASK_CONSTRUCTOR", va::CGAMETASK_CONSTRUCTOR),
            ("CONSTRUCT_DD_GAME", va::CONSTRUCT_DD_GAME),
            ("CONSTRUCT_DD_GAME_WRAPPER", va::CONSTRUCT_DD_GAME_WRAPPER),
            ("CONSTRUCT_PC_LANDSCAPE", va::CONSTRUCT_PC_LANDSCAPE),
            ("CONSTRUCT_DS_SOUND", va::CONSTRUCT_DS_SOUND),
            ("CREATE_EXPLOSION", va::CREATE_EXPLOSION),
            ("FRONTEND_CHANGE_SCREEN", va::FRONTEND_CHANGE_SCREEN),
        ];

        // Common x86 prologues: 55 (push ebp), 53 (push ebx), 56 (push esi),
        // 83 EC (sub esp), 8B (mov), 6A (push imm8), 81 (sub esp imm32)
        let valid_first_bytes: &[u8] = &[0x55, 0x53, 0x56, 0x57, 0x83, 0x8B, 0x6A, 0x81, 0xB8, 0x51, 0x52];

        for (name, addr) in functions {
            let first_byte = read_u8(*addr);
            let valid = valid_first_bytes.contains(&first_byte);
            result.check(
                name,
                valid,
                &format!("0x{:08X} starts with 0x{:02X} (valid prologue: {})", addr, first_byte, valid),
            );
        }
    }
}
```

**Step 2: Wire into run_validation**

```rust
fn run_validation() -> Result<(), Box<dyn std::error::Error>> {
    // Clear log file
    let _ = std::fs::write("OpenWA_validation.log", "");

    let _ = log_line("=== OpenWA Validator ===");
    let mut result = ValidationResult::new();

    validate_addresses(&mut result);

    result.summary();
    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build --target i686-pc-windows-msvc -p openwa-validator`
Expected: Compiles

**Step 4: Commit**

```
feat(validator): add address and vtable content validation
```

---

### Task 4: Add constructor hooks for struct validation

**Files:**
- Modify: `crates/openwa-validator/src/lib.rs`

Hook CTask and DDGameWrapper constructors to validate struct field offsets
when objects are actually constructed in the live game.

**Step 1: Define static detours and hook functions**

WA.exe constructors use `__thiscall` (ECX = this, args on stack).
`retour`'s `extern "thiscall"` maps the first parameter to ECX.

Note: DDGame constructor has many params. For validation we only need `this`
(ECX) and the DDGameWrapper pointer. We capture what we can.

```rust
use retour::static_detour;
use std::sync::atomic::{AtomicBool, Ordering};

// Track whether we've validated each struct (only do it once)
static CTASK_VALIDATED: AtomicBool = AtomicBool::new(false);
static DDGAME_WRAPPER_VALIDATED: AtomicBool = AtomicBool::new(false);

// CTask constructor: __thiscall with (this, parent_ptr, class_type)
// At 0x5625A0
type CTaskCtorFn = unsafe extern "thiscall" fn(this: u32, parent: u32, class_type: u32) -> u32;

static_detour! {
    static CTaskCtorHook: unsafe extern "thiscall" fn(u32, u32, u32) -> u32;
}

fn on_ctask_constructed(this: u32, parent: u32, class_type: u32) -> u32 {
    let ret = unsafe { CTaskCtorHook.call(this, parent, class_type) };

    if !CTASK_VALIDATED.swap(true, Ordering::Relaxed) {
        validate_ctask(this);
    }

    ret
}

fn validate_ctask(this: u32) {
    let mut result = ValidationResult::new();
    unsafe {
        // CTask.vtable at offset 0x00 should be CTASK_VTABLE
        // Note: subclasses override this, so we check it's a valid .text-pointing vtable
        let vtable = read_u32(this);
        let first_method = read_u32(vtable);
        result.check(
            "CTask+0x00 (vtable)",
            is_in_rdata(vtable),
            &format!("vtable ptr 0x{:08X} (in .rdata: {})", vtable, is_in_rdata(vtable)),
        );
        result.check(
            "CTask vtable[0] -> .text",
            is_in_text(first_method),
            &format!("first method 0x{:08X} (in .text: {})", first_method, is_in_text(first_method)),
        );

        // CTask._unknown_08 at offset 0x08 should be 0x10
        let field_08 = read_u32(this + 0x08);
        result.check(
            "CTask+0x08 (init value)",
            field_08 == 0x10,
            &format!("expected 0x10, got 0x{:X}", field_08),
        );

        // CTask.class_type at offset 0x20 should be a valid ClassType
        let class_type_val = read_u32(this + 0x20);
        result.check(
            "CTask+0x20 (class_type)",
            class_type_val < 200, // reasonable range for enum
            &format!("value {} (reasonable range: {})", class_type_val, class_type_val < 200),
        );
    }

    let _ = log_line("--- CTask struct validation ---");
    result.summary();
}

// DDGameWrapper constructor: __thiscall with (this) — simple
// At 0x56DEF0
type DDGameWrapperCtorFn = unsafe extern "thiscall" fn(this: u32) -> u32;

static_detour! {
    static DDGameWrapperCtorHook: unsafe extern "thiscall" fn(u32) -> u32;
}

fn on_ddgame_wrapper_constructed(this: u32) -> u32 {
    let ret = unsafe { DDGameWrapperCtorHook.call(this) };

    if !DDGAME_WRAPPER_VALIDATED.swap(true, Ordering::Relaxed) {
        // Delay validation slightly — the constructor sets fields but
        // DDGame is allocated later. We validate what we can now.
        validate_ddgame_wrapper_immediate(this);

        // Spawn a thread that waits briefly then checks DDGame pointer
        let wrapper_addr = this;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            validate_ddgame_wrapper_deferred(wrapper_addr);
        });
    }

    ret
}

fn validate_ddgame_wrapper_immediate(this: u32) {
    let mut result = ValidationResult::new();
    unsafe {
        // DDGameWrapper.vtable at +0x00
        let vtable = read_u32(this);
        result.check(
            "DDGameWrapper+0x00 (vtable)",
            vtable == va::DDGAME_WRAPPER_VTABLE,
            &format!("expected 0x{:08X}, got 0x{:08X}", va::DDGAME_WRAPPER_VTABLE, vtable),
        );

        // DDGameWrapper._field_4e0 at +0x4E0 should be -100 (0xFFFFFF9C)
        let field_4e0 = read_u32(this + 0x4E0);
        result.check(
            "DDGameWrapper+0x4E0 (init -100)",
            field_4e0 == 0xFFFFFF9C,
            &format!("expected 0xFFFFFF9C, got 0x{:08X}", field_4e0),
        );
    }

    let _ = log_line("--- DDGameWrapper struct validation (immediate) ---");
    result.summary();
}

fn validate_ddgame_wrapper_deferred(this: u32) {
    let mut result = ValidationResult::new();
    unsafe {
        // DDGameWrapper.ddgame at +0x488
        let ddgame_ptr = read_u32(this + 0x488);
        result.check(
            "DDGameWrapper+0x488 (ddgame ptr)",
            ddgame_ptr != 0,
            &format!("0x{:08X} (non-null: {})", ddgame_ptr, ddgame_ptr != 0),
        );

        if ddgame_ptr != 0 {
            // DDGame.landscape at +0x20 should match wrapper.landscape at +0x4CC
            let ddgame_landscape = read_u32(ddgame_ptr + 0x20);
            let wrapper_landscape = read_u32(this + 0x4CC);
            result.check(
                "DDGame+0x20 == DDGameWrapper+0x4CC (landscape)",
                ddgame_landscape == wrapper_landscape,
                &format!(
                    "DDGame: 0x{:08X}, Wrapper: 0x{:08X}",
                    ddgame_landscape, wrapper_landscape
                ),
            );

            // DDGame+0x7EFC should be 1 (init value)
            let field_7efc = read_u32(ddgame_ptr + 0x7EFC);
            result.check(
                "DDGame+0x7EFC (init 1)",
                field_7efc == 1,
                &format!("expected 1, got {}", field_7efc),
            );

            // DDGame.display_gfx at +0x138 — if set, vtable should be DISPLAY_GFX_VTABLE
            let display_gfx = read_u32(ddgame_ptr + 0x138);
            if display_gfx != 0 {
                let dgfx_vt = read_u32(display_gfx);
                result.check(
                    "DDGame+0x138 display_gfx vtable",
                    dgfx_vt == va::DISPLAY_GFX_VTABLE,
                    &format!("expected 0x{:08X}, got 0x{:08X}", va::DISPLAY_GFX_VTABLE, dgfx_vt),
                );
            }

            // DDGame.task_state_machine at +0x380 — if set, vtable should be TASK_STATE_MACHINE_VTABLE
            let tsm = read_u32(ddgame_ptr + 0x380);
            if tsm != 0 {
                let tsm_vt = read_u32(tsm);
                result.check(
                    "DDGame+0x380 task_state_machine vtable",
                    tsm_vt == va::TASK_STATE_MACHINE_VTABLE,
                    &format!("expected 0x{:08X}, got 0x{:08X}", va::TASK_STATE_MACHINE_VTABLE, tsm_vt),
                );
            }
        }

        // DDGameWrapper.gfx_handler_0 at +0x4C0 — vtable should be GFX_HANDLER_VTABLE
        let gfx0 = read_u32(this + 0x4C0);
        if gfx0 != 0 {
            let gfx0_vt = read_u32(gfx0);
            result.check(
                "DDGameWrapper+0x4C0 gfx_handler_0 vtable",
                gfx0_vt == va::GFX_HANDLER_VTABLE,
                &format!("expected 0x{:08X}, got 0x{:08X}", va::GFX_HANDLER_VTABLE, gfx0_vt),
            );
        }

        // DDGameWrapper.landscape at +0x4CC — vtable should be PC_LANDSCAPE_VTABLE
        let landscape = read_u32(this + 0x4CC);
        if landscape != 0 {
            let land_vt = read_u32(landscape);
            result.check(
                "DDGameWrapper+0x4CC landscape vtable",
                land_vt == va::PC_LANDSCAPE_VTABLE,
                &format!("expected 0x{:08X}, got 0x{:08X}", va::PC_LANDSCAPE_VTABLE, land_vt),
            );
        }
    }

    let _ = log_line("--- DDGameWrapper/DDGame struct validation (deferred) ---");
    result.summary();
}
```

**Step 2: Install hooks in run_validation**

```rust
fn run_validation() -> Result<(), Box<dyn std::error::Error>> {
    let _ = std::fs::write("OpenWA_validation.log", "");
    let _ = log_line("=== OpenWA Validator ===");

    let mut result = ValidationResult::new();
    validate_addresses(&mut result);
    result.summary();

    // Install constructor hooks
    let _ = log_line("\nInstalling constructor hooks...");

    unsafe {
        let ctask_ctor: CTaskCtorFn = std::mem::transmute(va::CTASK_CONSTRUCTOR as usize);
        CTaskCtorHook
            .initialize(ctask_ctor, |this, parent, class_type| {
                on_ctask_constructed(this, parent, class_type)
            })?
            .enable()?;
        let _ = log_line("[HOOK] CTask constructor hooked at 0x5625A0");

        let wrapper_ctor: DDGameWrapperCtorFn =
            std::mem::transmute(va::CONSTRUCT_DD_GAME_WRAPPER as usize);
        DDGameWrapperCtorHook
            .initialize(wrapper_ctor, |this| {
                on_ddgame_wrapper_constructed(this)
            })?
            .enable()?;
        let _ = log_line("[HOOK] DDGameWrapper constructor hooked at 0x56DEF0");
    }

    let _ = log_line("Hooks installed. Start a game to trigger struct validation.\n");
    Ok(())
}
```

**Step 3: Verify it compiles**

Run: `cargo build --target i686-pc-windows-msvc -p openwa-validator`
Expected: Compiles

**Step 4: Commit**

```
feat(validator): add constructor hooks for CTask and DDGameWrapper validation
```

---

### Task 5: Manual testing with WA.exe

**Step 1: Build release**

Run: `cargo build --target i686-pc-windows-msvc -p openwa-validator --release`

**Step 2: Deploy to game directory**

Copy `target/i686-pc-windows-msvc/release/openwa_validator.dll` to
`I:\games\SteamLibrary\steamapps\common\Worms Armageddon\wkOpenWAValidator.dll`

Note: WormKit's HookLib auto-loads any `wk*.dll` in the game directory.
If HookLib isn't present, the DLL won't load. In that case, we can use
a simple injector or rename to test with manual LoadLibrary.

**Step 3: Launch the game and start a match**

1. Launch WA.exe (via Steam or directly)
2. Start a single-player deathmatch (to trigger DDGame/CTask constructors)
3. Exit the game

**Step 4: Check the log**

Read `I:\games\SteamLibrary\steamapps\common\Worms Armageddon\OpenWA_validation.log`

Expected: A mix of PASS/FAIL lines showing which addresses and struct offsets
are correct. Any FAILs indicate bugs in our `openwa-types` definitions.

**Step 5: Fix any failures**

If struct offsets are wrong, update `openwa-types` accordingly and re-run.
This is the core value of the validator — it tells us exactly which
fields are at wrong offsets.

**Step 6: Commit any fixes**

```
fix(types): correct struct offsets based on runtime validation
```

---

### Task 6: Add offset_of validation for Rust struct layouts

**Files:**
- Modify: `crates/openwa-validator/src/lib.rs`

Use `core::mem::offset_of!` (stable since Rust 1.77) to verify our `#[repr(C)]`
struct field offsets match the documented byte offsets. This catches mistakes
where padding or field ordering is wrong, without needing live game memory.

**Step 1: Add compile-time / load-time offset checks**

```rust
use openwa_types::task::{CTask, CGameTask};
use openwa_types::ddgame::DDGame;
use openwa_types::ddgame_wrapper::DDGameWrapper;

fn validate_struct_offsets(result: &mut ValidationResult) {
    // CTask offsets
    let checks: &[(&str, usize, usize)] = &[
        ("CTask::vtable", core::mem::offset_of!(CTask, vtable), 0x00),
        ("CTask::parent", core::mem::offset_of!(CTask, parent), 0x04),
        ("CTask::_unknown_08", core::mem::offset_of!(CTask, _unknown_08), 0x08),
        ("CTask::class_type", core::mem::offset_of!(CTask, class_type), 0x20),

        // CGameTask offsets
        ("CGameTask::base", core::mem::offset_of!(CGameTask, base), 0x00),
        ("CGameTask::pos_x", core::mem::offset_of!(CGameTask, pos_x), 0x84),
        ("CGameTask::pos_y", core::mem::offset_of!(CGameTask, pos_y), 0x88),
        ("CGameTask::speed_x", core::mem::offset_of!(CGameTask, speed_x), 0x90),
        ("CGameTask::speed_y", core::mem::offset_of!(CGameTask, speed_y), 0x94),
        ("CGameTask::vtable2", core::mem::offset_of!(CGameTask, vtable2), 0xE8),

        // DDGame offsets
        ("DDGame::landscape", core::mem::offset_of!(DDGame, landscape), 0x20),
        ("DDGame::game_state", core::mem::offset_of!(DDGame, game_state), 0x24),
        ("DDGame::arrow_sprites", core::mem::offset_of!(DDGame, arrow_sprites), 0x38),
        ("DDGame::arrow_gfxdirs", core::mem::offset_of!(DDGame, arrow_gfxdirs), 0xB8),
        ("DDGame::display_gfx", core::mem::offset_of!(DDGame, display_gfx), 0x138),
        ("DDGame::task_state_machine", core::mem::offset_of!(DDGame, task_state_machine), 0x380),
        ("DDGame::sprite_regions", core::mem::offset_of!(DDGame, sprite_regions), 0x46C),
        ("DDGame::coord_list", core::mem::offset_of!(DDGame, coord_list), 0x50C),

        // DDGameWrapper offsets
        ("DDGameWrapper::vtable", core::mem::offset_of!(DDGameWrapper, vtable), 0x00),
        ("DDGameWrapper::ddgame", core::mem::offset_of!(DDGameWrapper, ddgame), 0x488),
        ("DDGameWrapper::gfx_handler_0", core::mem::offset_of!(DDGameWrapper, gfx_handler_0), 0x4C0),
        ("DDGameWrapper::landscape", core::mem::offset_of!(DDGameWrapper, landscape), 0x4CC),
        ("DDGameWrapper::display", core::mem::offset_of!(DDGameWrapper, display), 0x4D0),
    ];

    for (name, actual, expected) in checks {
        result.check(
            &format!("offset_of {}", name),
            *actual == *expected,
            &format!("expected 0x{:X}, got 0x{:X}", expected, actual),
        );
    }
}
```

**Step 2: Call from run_validation, before the hook installation**

Add after `validate_addresses`:

```rust
let _ = log_line("\n--- Struct offset validation (compile-time layout) ---");
validate_struct_offsets(&mut result);
```

**Step 3: Verify it compiles**

Run: `cargo build --target i686-pc-windows-msvc -p openwa-validator`

**Step 4: Commit**

```
feat(validator): add offset_of! checks for Rust struct layouts
```

---

## Deployment Checklist

1. `rustup target add i686-pc-windows-msvc`
2. `cargo build --target i686-pc-windows-msvc -p openwa-validator --release`
3. Copy DLL to game dir as `wkOpenWAValidator.dll`
4. Ensure WormKit HookLib is in game dir (or use alternative injection)
5. Launch game, start a match, exit
6. Read `OpenWA_validation.log`
