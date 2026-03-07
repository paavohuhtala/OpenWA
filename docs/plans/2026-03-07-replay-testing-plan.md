# Replay-Based Automated Testing Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Enable automated validation and RE discovery by launching WA.exe with replay files, auto-capturing memory dumps, and presenting results via a Claude Code skill.

**Architecture:** A PowerShell script orchestrates build/deploy/launch/log-copy. The validator DLL detects `OPENWA_REPLAY_TEST` env var and runs a single auto-capture sequence (wait 30s, dump all, ExitProcess). A Claude Code skill wraps the script and reads logs.

**Tech Stack:** PowerShell, Rust (openwa-validator), Claude Code skills (markdown)

---

### Task 1: Add `testdata/logs/` to `.gitignore`

**Files:**
- Modify: `.gitignore`

**Step 1: Add the gitignore entry**

Append to `.gitignore`:
```
testdata/logs/
```

**Step 2: Commit**

```bash
git add .gitignore
git commit -m "chore: gitignore testdata/logs/"
```

---

### Task 2: Add auto-capture mode to validator DLL

**Files:**
- Modify: `crates/openwa-validator/Cargo.toml` (add `Win32_System_Threading` feature)
- Modify: `crates/openwa-validator/src/lib.rs` (add auto-capture branch in `run_validation()`)

**Step 1: Add `Win32_System_Threading` feature to Cargo.toml**

In `crates/openwa-validator/Cargo.toml`, add `"Win32_System_Threading"` to the windows-sys features list. This is needed for `ExitProcess`.

```toml
windows-sys = { version = "0.59", features = [
    "Win32_System_Memory",
    "Win32_System_LibraryLoader",
    "Win32_UI_Input_KeyboardAndMouse",
    "Win32_System_Threading",
] }
```

**Step 2: Modify `run_validation()` in `crates/openwa-validator/src/lib.rs`**

After the static validation and hook installation (after step 6 in current code, line ~814), add a branch that checks the env var. Replace the current deferred/hotkey thread spawning (lines 816-856) with:

```rust
    // 7. Mode-dependent: auto-capture vs interactive
    let auto_mode = std::env::var("OPENWA_REPLAY_TEST").is_ok();

    if auto_mode {
        let _ = log_line("");
        let _ = log_line("--- Auto-Capture Mode (OPENWA_REPLAY_TEST) ---");
        std::thread::spawn(move || {
            let _ = log_line("  Waiting 30s for replay to reach gameplay...");
            std::thread::sleep(std::time::Duration::from_secs(30));

            let _ = log_line("  Running deferred global validation...");
            deferred_global_validation();

            let _ = log_line("  Running team block dump...");
            dump_team_blocks();

            let _ = log_line("  Running landscape dump...");
            dump_landscape();

            let _ = log_line("");
            let _ = log_line("--- Auto-capture complete, exiting ---");

            unsafe {
                windows_sys::Win32::System::Threading::ExitProcess(0);
            }
        });
    } else {
        // Existing interactive mode: deferred timers + hotkey listener
        let _ = log_line("");
        let _ = log_line("--- Interactive Mode (deferred polling + hotkeys) ---");
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(10));
            deferred_global_validation();
        });
        let _ = log_line("  Polling thread started (10s delay).");

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            dump_team_blocks();
        });
        let _ = log_line("  Team block dump thread started (30s delay).");

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(15));
            dump_landscape();
        });
        let _ = log_line("  Landscape dump thread started (15s delay).");

        std::thread::spawn(|| {
            const VK_F9: i32 = 0x78;
            const VK_F10: i32 = 0x79;
            let _ = log_line("  Hotkey listener started (F9=team blocks, F10=landscape).");
            loop {
                std::thread::sleep(std::time::Duration::from_millis(100));
                unsafe {
                    if GetAsyncKeyState(VK_F9) & 1 != 0 {
                        dump_team_blocks();
                    }
                    if GetAsyncKeyState(VK_F10) & 1 != 0 {
                        dump_landscape();
                    }
                }
            }
        });
    }
```

**Step 3: Verify it compiles**

Run: `cargo build --release -p openwa-validator`
Expected: Compiles successfully.

**Step 4: Commit**

```bash
git add crates/openwa-validator/Cargo.toml crates/openwa-validator/src/lib.rs
git commit -m "feat: add auto-capture mode to validator DLL via OPENWA_REPLAY_TEST env var"
```

---

### Task 3: Create the PowerShell orchestration script

**Files:**
- Create: `replay-test.ps1`

**Step 1: Create `replay-test.ps1`**

```powershell
# replay-test.ps1 - Automated replay-based testing
# Usage: powershell -File replay-test.ps1 [replay-file]

param(
    [string]$ReplayFile = "testdata\replays\bots.WAgame"
)

$ErrorActionPreference = "Stop"

$gameDir = "I:\games\SteamLibrary\steamapps\common\Worms Armageddon"
$waExe = "$gameDir\WA.exe"
$src = "target\i686-pc-windows-msvc\release"
$logDir = "testdata\logs"

# Resolve replay file to absolute path
if (-not [System.IO.Path]::IsPathRooted($ReplayFile)) {
    $ReplayFile = Join-Path (Get-Location) $ReplayFile
}

if (-not (Test-Path $ReplayFile)) {
    Write-Error "Replay file not found: $ReplayFile"
    exit 1
}

# 1. Build
Write-Host "Building..." -ForegroundColor Cyan
cargo build --release -p openwa-wormkit -p openwa-validator
if ($LASTEXITCODE -ne 0) { exit 1 }

# 2. Deploy
Write-Host "Deploying to $gameDir..." -ForegroundColor Cyan
Copy-Item "$src\openwa_wormkit.dll" "$gameDir\wkOpenWA.dll"
Copy-Item "$src\openwa_validator.dll" "$gameDir\wkOpenWAValidator.dll"

# 3. Clear old logs
Remove-Item "$gameDir\OpenWA.log" -ErrorAction SilentlyContinue
Remove-Item "$gameDir\OpenWA_validation.log" -ErrorAction SilentlyContinue

# 4. Launch WA.exe with replay, env var set
Write-Host "Launching WA.exe with replay: $ReplayFile" -ForegroundColor Cyan
Write-Host "  (Auto-capture mode: will exit after 30s)" -ForegroundColor Yellow

$env:OPENWA_REPLAY_TEST = "1"
$proc = Start-Process -FilePath $waExe -ArgumentList "`"$ReplayFile`"" -PassThru
$proc.WaitForExit()
$env:OPENWA_REPLAY_TEST = $null

Write-Host "WA.exe exited with code $($proc.ExitCode)" -ForegroundColor Cyan

# 5. Copy logs back
if (-not (Test-Path $logDir)) {
    New-Item -ItemType Directory -Path $logDir | Out-Null
}

$timestamp = Get-Date -Format "yyyy-MM-dd_HH-mm-ss"

if (Test-Path "$gameDir\OpenWA_validation.log") {
    Copy-Item "$gameDir\OpenWA_validation.log" "$logDir\validation_$timestamp.log"
    Copy-Item "$gameDir\OpenWA_validation.log" "$logDir\validation_latest.log"
    Write-Host "Validation log copied." -ForegroundColor Green
} else {
    Write-Host "WARNING: No validation log found!" -ForegroundColor Red
}

if (Test-Path "$gameDir\OpenWA.log") {
    Copy-Item "$gameDir\OpenWA.log" "$logDir\openwa_$timestamp.log"
    Copy-Item "$gameDir\OpenWA.log" "$logDir\openwa_latest.log"
    Write-Host "OpenWA log copied." -ForegroundColor Green
}

# 6. Print summary
Write-Host ""
Write-Host "=== Validation Summary ===" -ForegroundColor Cyan
if (Test-Path "$logDir\validation_latest.log") {
    $content = Get-Content "$logDir\validation_latest.log" -Raw
    $passes = ([regex]::Matches($content, "\[PASS\]")).Count
    $fails = ([regex]::Matches($content, "\[FAIL\]")).Count
    Write-Host "  PASS: $passes" -ForegroundColor Green
    Write-Host "  FAIL: $fails" -ForegroundColor $(if ($fails -gt 0) { "Red" } else { "Green" })

    if ($content -match "Auto-capture complete") {
        Write-Host "  Auto-capture: completed successfully" -ForegroundColor Green
    } else {
        Write-Host "  Auto-capture: NOT found in log (may not have reached gameplay)" -ForegroundColor Yellow
    }
}
Write-Host "  Logs: $logDir\" -ForegroundColor Gray
```

**Step 2: Test the script manually**

Run: `powershell -File replay-test.ps1`

Expected: Builds, deploys, launches WA.exe with `bots.WAgame`, WA exits after ~30s, logs appear in `testdata/logs/`.

**Step 3: Commit**

```bash
git add replay-test.ps1
git commit -m "feat: add replay-test.ps1 orchestration script"
```

---

### Task 4: Create the Claude Code skill

**Files:**
- Create: `.claude/skills/replay-test.md`

**Step 1: Create skill directory and file**

Create `.claude/skills/replay-test.md`:

```markdown
---
name: replay-test
description: Run automated replay-based testing - builds, deploys, launches WA.exe with a replay file, captures validation logs, and presents results. Use when you need to validate struct layouts, hooks, or game state against live WA.exe.
user_invocable: true
---

# Replay Test

Run the replay-based automated testing pipeline.

## Steps

1. Run the replay test script:

```bash
powershell -File replay-test.ps1
```

This will:
- Build openwa-wormkit and openwa-validator
- Deploy DLLs to the WA game directory
- Launch WA.exe with the default replay file (testdata/replays/bots.WAgame)
- Wait for auto-capture (30s) and process exit
- Copy logs to testdata/logs/

2. Read the validation log and present results:

Read `testdata/logs/validation_latest.log` and summarize:
- Total PASS/FAIL counts
- Any [FAIL] lines (quote them exactly)
- Whether auto-capture completed
- Key data from dumps (team blocks, landscape) if present

3. If the user also has `testdata/logs/openwa_latest.log`, read it and note any errors or interesting hook activity.

## Notes

- The script takes ~40 seconds total (build + 30s auto-capture wait)
- If WA.exe doesn't exit, the validator DLL may not have loaded. Check that wkOpenWAValidator.dll exists in the game directory.
- Replay file can be changed by editing the script invocation: `powershell -File replay-test.ps1 path\to\other.WAgame`
```

**Step 2: Commit**

```bash
git add .claude/skills/replay-test.md
git commit -m "feat: add /replay-test Claude Code skill"
```

---

### Task 5: End-to-end test

**Step 1: Run the skill**

Invoke `/replay-test` and verify the full pipeline works:
- Build succeeds
- WA.exe launches with replay
- WA.exe exits after ~30s
- Logs are copied to `testdata/logs/`
- Validation summary shows PASS/FAIL counts
- Auto-capture complete message is present in log

**Step 2: Review logs for correctness**

Check that:
- Static validation (vtables, prologues, struct offsets) all PASS
- Deferred global validation ran (DDGameWrapper vtable check)
- Team block dump shows team data (worm names, health, state)
- Landscape dump shows level data (dimensions, layers)

**Step 3: Fix any issues found, commit fixes**
