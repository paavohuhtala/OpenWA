//! Headless replay test runner with concurrent execution.
//!
//! Discovers all `testdata/replays/*.WAgame` files with matching `*_expected.log`,
//! builds the DLL + launcher, then runs each replay through WA.exe's `/getlog` mode
//! and compares output byte-for-byte.

use std::env;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::IsTerminal;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

// ─── Configuration ──────────────────────────────────────────────────────────

const DEFAULT_JOBS: usize = 4;
const TIMEOUT_SECS: u64 = 120;
const REPLAYS_DIR: &str = "testdata/replays";
const RUNS_DIR: &str = "testdata/runs";


// ─── Types ──────────────────────────────────────────────────────────────────

struct TestCase {
    name: String,
    replay_path: PathBuf,
    expected_log: PathBuf,
    output_log: PathBuf,
}

#[derive(Clone)]
struct CrashInfo {
    exit_code: u32,
    name: &'static str,
    errorlog_content: Option<String>,
}

#[derive(Clone)]
struct TestResult {
    name: String,
    passed: bool,
    duration: Duration,
    diff_lines: Vec<String>,
    error: Option<String>,
    crashed: Option<CrashInfo>,
}

struct Args {
    filter: Option<String>,
    jobs: usize,
    no_build: bool,
    wa_path: Option<PathBuf>,
}

// ─── Argument parsing ───────────────────────────────────────────────────────

fn parse_args() -> Args {
    let mut args = Args {
        filter: None,
        jobs: DEFAULT_JOBS,
        no_build: false,
        wa_path: None,
    };

    let argv: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-j" | "--jobs" => {
                i += 1;
                if i < argv.len() {
                    args.jobs = argv[i].parse().unwrap_or(DEFAULT_JOBS);
                }
            }
            "--no-build" => args.no_build = true,
            "--wa-path" => {
                i += 1;
                if i < argv.len() {
                    args.wa_path = Some(PathBuf::from(&argv[i]));
                }
            }
            s if !s.starts_with('-') => {
                args.filter = Some(s.to_string());
            }
            other => {
                eprintln!("Unknown argument: {other}");
                eprintln!("Usage: openwa-test [filter] [-j N] [--no-build] [--wa-path PATH]");
                std::process::exit(1);
            }
        }
        i += 1;
    }

    args
}

// ─── Test discovery ─────────────────────────────────────────────────────────

fn discover_tests(filter: Option<&str>) -> Vec<TestCase> {
    let replays_dir = Path::new(REPLAYS_DIR);
    let mut tests = Vec::new();

    let entries = match fs::read_dir(replays_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Cannot read {REPLAYS_DIR}: {e}");
            return tests;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("WAgame") {
            continue;
        }

        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };

        if let Some(filter) = filter {
            if !stem.contains(filter) {
                continue;
            }
        }

        let expected = replays_dir.join(format!("{stem}_expected.log"));
        if !expected.exists() {
            continue; // Skip replays without expected logs
        }

        // WA.exe runs in the game directory, so replay/log paths must be absolute.
        // Strip \\?\ UNC prefix that canonicalize adds on Windows — WA.exe can't handle it.
        let replay_abs = strip_unc(fs::canonicalize(&path).unwrap_or(path.clone()));
        let expected_abs = strip_unc(fs::canonicalize(&expected).unwrap_or(expected.clone()));
        let output_abs = replay_abs.with_extension("log");

        tests.push(TestCase {
            name: stem,
            replay_path: replay_abs,
            expected_log: expected_abs,
            output_log: output_abs,
        });
    }

    tests.sort_by(|a, b| a.name.cmp(&b.name));
    tests
}

// ─── WA.exe location ────────────────────────────────────────────────────────

fn find_wa_exe(override_path: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = override_path {
        if p.exists() {
            return Some(p.to_path_buf());
        }
    }
    openwa_config::find_wa_dir().map(|d| d.join("WA.exe"))
}

fn find_launcher() -> Option<PathBuf> {
    // Look for the launcher in the release build output
    let p = PathBuf::from("target/i686-pc-windows-msvc/release/openwa-launcher.exe");
    if p.exists() {
        return Some(p);
    }
    let p = PathBuf::from("target/release/openwa-launcher.exe");
    if p.exists() {
        return Some(p);
    }
    None
}

// ─── Build ──────────────────────────────────────────────────────────────────

fn build() -> Result<Duration, String> {
    let start = Instant::now();
    let status = Command::new("cargo")
        .args([
            "build",
            "--release",
            "-p",
            "openwa-wormkit",
            "-p",
            "openwa-launcher",
        ])
        .status()
        .map_err(|e| format!("Failed to run cargo: {e}"))?;

    if !status.success() {
        return Err("Build failed".to_string());
    }

    Ok(start.elapsed())
}

// ─── Single test execution ──────────────────────────────────────────────────

fn run_test(test: &TestCase, launcher: &Path, wa_exe: &Path, run_dir: &Path) -> TestResult {
    let start = Instant::now();

    // Remove stale output log
    let _ = fs::remove_file(&test.output_log);

    // Per-instance log paths
    let openwa_log = run_dir.join(format!("{}.openwa.log", test.name));
    let errorlog_path = run_dir.join(format!("{}.errorlog.txt", test.name));

    let result = Command::new(launcher)
        .arg(wa_exe)
        .arg("/getlog")
        .arg(&test.replay_path)
        .env("OPENWA_HEADLESS", "1")
        .env("OPENWA_LOG_PATH", &openwa_log)
        .env("OPENWA_ERRORLOG_PATH", &errorlog_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    let duration = start.elapsed();

    match result {
        Err(e) => TestResult {
            name: test.name.clone(),
            passed: false,
            duration,
            diff_lines: Vec::new(),
            error: Some(format!("Failed to launch: {e}")),
            crashed: None,
        },
        Ok(status) => {
            let exit_code = status.code().unwrap_or(0);

            if duration >= Duration::from_secs(TIMEOUT_SECS) {
                return TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration,
                    diff_lines: Vec::new(),
                    error: Some("Timeout".to_string()),
                    crashed: None,
                };
            }

            // Check for crash: either an NTSTATUS exit code (negative i32)
            // or the presence of ERRORLOG.TXT (WA.exe's SEH handler catches
            // exceptions and exits cleanly, so exit code may be 0).
            // Use lossy conversion — ERRORLOG.TXT may contain binary
            // memory dump data that isn't valid UTF-8.
            let errorlog_content = fs::read(&errorlog_path)
                .ok()
                .filter(|b| !b.is_empty())
                .map(|b| String::from_utf8_lossy(&b).into_owned());

            if is_crash_exit_code(exit_code) || errorlog_content.is_some() {
                let unsigned = exit_code as u32;
                let name = if is_crash_exit_code(exit_code) {
                    ntstatus_name(unsigned)
                } else {
                    // Parse exception from ERRORLOG first line, e.g.
                    // "WA caused an Access Violation (0xc0000005)"
                    parse_errorlog_exception(errorlog_content.as_deref())
                };
                return TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration,
                    diff_lines: Vec::new(),
                    error: None,
                    crashed: Some(CrashInfo {
                        exit_code: unsigned,
                        name,
                        errorlog_content,
                    }),
                };
            }

            // Copy output log to run dir
            if test.output_log.exists() {
                let dest = run_dir.join(format!("{}.log", test.name));
                let _ = fs::copy(&test.output_log, &dest);
            }

            // Compare output
            if !test.output_log.exists() {
                return TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration,
                    diff_lines: Vec::new(),
                    error: Some(format!("No output log generated (exit code: {exit_code})")),
                    crashed: None,
                };
            }

            // Normalize CRLF → LF for comparison (WA mixes line endings)
            let expected = normalize_crlf(&fs::read(&test.expected_log).unwrap_or_default());
            let actual = normalize_crlf(&fs::read(&test.output_log).unwrap_or_default());

            if expected == actual {
                // Clean up the output log on success
                let _ = fs::remove_file(&test.output_log);
                TestResult {
                    name: test.name.clone(),
                    passed: true,
                    duration,
                    diff_lines: Vec::new(),
                    error: None,
                    crashed: None,
                }
            } else {
                let mut diff = compute_diff(&expected, &actual);
                if diff.is_empty() {
                    diff.push(format!(
                        "  (byte-level difference: expected {} bytes, actual {} bytes)",
                        expected.len(),
                        actual.len()
                    ));
                }
                let _ = fs::remove_file(&test.output_log);
                TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration,
                    diff_lines: diff,
                    error: None,
                    crashed: None,
                }
            }
        }
    }
}

/// Compute a simple line diff (first 10 differing lines).
fn compute_diff(expected: &[u8], actual: &[u8]) -> Vec<String> {
    let exp_str = String::from_utf8_lossy(expected);
    let act_str = String::from_utf8_lossy(actual);
    let exp_lines: Vec<&str> = exp_str.lines().collect();
    let act_lines: Vec<&str> = act_str.lines().collect();

    let mut diffs = Vec::new();
    let max = exp_lines.len().max(act_lines.len());

    for i in 0..max {
        let e = exp_lines.get(i).copied().unwrap_or("");
        let a = act_lines.get(i).copied().unwrap_or("");
        if e != a {
            if !e.is_empty() {
                diffs.push(format!("  - expected: {e}"));
            }
            if !a.is_empty() {
                diffs.push(format!("  + actual:   {a}"));
            }
            if diffs.len() >= 10 {
                diffs.push("  ... (truncated)".to_string());
                break;
            }
        }
    }

    diffs
}

// ─── Thread pool ────────────────────────────────────────────────────────────

fn run_tests_parallel(
    tests: Vec<TestCase>,
    jobs: usize,
    launcher: &Path,
    wa_exe: &Path,
    run_dir: &Path,
) -> Vec<TestResult> {
    let (tx, rx) = mpsc::channel::<(usize, TestResult)>();
    let (work_tx, work_rx) = mpsc::channel::<(usize, TestCase)>();
    let work_rx = std::sync::Arc::new(std::sync::Mutex::new(work_rx));

    let launcher = launcher.to_path_buf();
    let wa_exe = wa_exe.to_path_buf();
    let run_dir = run_dir.to_path_buf();

    // Spawn worker threads
    let mut handles = Vec::new();
    for _ in 0..jobs.min(tests.len()) {
        let rx = work_rx.clone();
        let tx = tx.clone();
        let launcher = launcher.clone();
        let wa_exe = wa_exe.clone();
        let run_dir = run_dir.clone();

        handles.push(thread::spawn(move || {
            loop {
                let item = {
                    let guard = rx.lock().unwrap();
                    guard.recv()
                    // MutexGuard drops here — lock released before running the test
                };
                match item {
                    Ok((idx, test)) => {
                        let result = run_test(&test, &launcher, &wa_exe, &run_dir);
                        let _ = tx.send((idx, result));
                    }
                    Err(_) => break,
                }
            }
        }));
    }
    drop(tx); // Drop our sender so rx closes when all workers finish

    // Small stagger between launches to reduce startup races on shared
    // resources (thm.prv, custom.dat) that can't be safely redirected.
    for (idx, test) in tests.into_iter().enumerate() {
        if idx > 0 {
            thread::sleep(Duration::from_millis(50));
        }
        let _ = work_tx.send((idx, test));
    }
    drop(work_tx); // Signal no more work

    // Collect results
    let mut results: Vec<Option<TestResult>> = vec![None; handles.len() + 100]; // oversize
    let mut count = 0;
    for (idx, result) in rx {
        if idx >= results.len() {
            results.resize(idx + 1, None);
        }
        // Print result as it arrives
        print_result(&result);
        results[idx] = Some(result);
        count += 1;
    }

    for h in handles {
        let _ = h.join();
    }

    results.into_iter().take(count).flatten().collect()
}

// ─── Output ─────────────────────────────────────────────────────────────────

fn use_color() -> bool {
    std::io::stdout().is_terminal()
}

fn green(text: &str) -> String {
    if use_color() {
        format!("\x1b[32m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn red(text: &str) -> String {
    if use_color() {
        format!("\x1b[31m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

fn print_result(result: &TestResult) {
    let status = if result.passed {
        green("  PASS")
    } else if result.crashed.is_some() {
        red(" CRASH")
    } else {
        red("  FAIL")
    };
    let secs = result.duration.as_secs_f64();
    println!("{status}  {:<28} ({secs:.1}s)", result.name);

    if let Some(crash) = &result.crashed {
        println!(
            "        {} (exit code: 0x{:08X})",
            crash.name, crash.exit_code
        );
        if let Some(content) = &crash.errorlog_content {
            println!("        --- ERRORLOG.TXT ---");
            let lines: Vec<&str> = content.lines().collect();
            for line in lines.iter().take(20) {
                println!("        | {line}");
            }
            if lines.len() > 20 {
                println!("        | ... ({} lines truncated)", lines.len() - 20);
            }
        }
    }
    if let Some(err) = &result.error {
        println!("        {err}");
    }
    for line in &result.diff_lines {
        println!("        {line}");
    }
}

fn print_summary(results: &[TestResult], wall_time: Duration) {
    let passed = results.iter().filter(|r| r.passed).count();
    let crashed = results.iter().filter(|r| r.crashed.is_some()).count();
    let failed = results.len() - passed;
    let cpu_time: f64 = results.iter().map(|r| r.duration.as_secs_f64()).sum();
    let wall = wall_time.as_secs_f64();

    println!();
    if failed == 0 {
        let msg = format!("{} tests: all passed", results.len());
        println!("{} (wall {wall:.1}s, cpu {cpu_time:.1}s)", green(&msg));
    } else {
        let crash_info = if crashed > 0 {
            format!(" ({crashed} crashed)")
        } else {
            String::new()
        };
        let msg = format!(
            "{} tests: {passed} passed, {failed} failed{crash_info}",
            results.len()
        );
        println!("{} (wall {wall:.1}s, cpu {cpu_time:.1}s)", red(&msg));
    }
}

fn write_summary(results: &[TestResult], wall_time: Duration, path: &Path) {
    let mut s = String::new();
    for r in results {
        let status = if r.passed {
            "PASS"
        } else if r.crashed.is_some() {
            "CRASH"
        } else {
            "FAIL"
        };
        let _ = writeln!(
            s,
            "{status}  {:<28} ({:.1}s)",
            r.name,
            r.duration.as_secs_f64()
        );
        if let Some(crash) = &r.crashed {
            let _ = writeln!(
                s,
                "        {} (exit code: 0x{:08X})",
                crash.name, crash.exit_code
            );
            if let Some(content) = &crash.errorlog_content {
                let _ = writeln!(s, "        --- ERRORLOG.TXT ---");
                for line in content.lines().take(20) {
                    let _ = writeln!(s, "        | {line}");
                }
                let line_count = content.lines().count();
                if line_count > 20 {
                    let _ = writeln!(s, "        | ... ({} lines truncated)", line_count - 20);
                }
            }
        }
        if let Some(err) = &r.error {
            let _ = writeln!(s, "        {err}");
        }
        for line in &r.diff_lines {
            let _ = writeln!(s, "        {line}");
        }
    }
    let passed = results.iter().filter(|r| r.passed).count();
    let crashed = results.iter().filter(|r| r.crashed.is_some()).count();
    let failed = results.len() - passed;
    let crash_info = if crashed > 0 {
        format!(" ({crashed} crashed)")
    } else {
        String::new()
    };
    let _ = writeln!(
        s,
        "\n{} tests: {passed} passed, {failed} failed{crash_info} (wall {:.1}s)",
        results.len(),
        wall_time.as_secs_f64()
    );

    let _ = fs::write(path, s);
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Clean up temp files and directories left by the file isolation hook.
///
/// The DLL creates `.openwa_tmp/{pid}/` directories and cleans them on exit,
/// but crashed processes may leave orphans. Also cleans stale ERRORLOG/CRASH files.
fn cleanup_temp_files(wa_exe: &Path) {
    let game_dir = match wa_exe.parent() {
        Some(d) => d,
        None => return,
    };

    // Remove the .openwa_tmp directory tree (all per-PID temp dirs)
    let tmp_dir = game_dir.join(".openwa_tmp");
    if tmp_dir.exists() {
        let _ = fs::remove_dir_all(&tmp_dir);
    }

    // Clean up any stale ERRORLOG.TXT / CRASH.DMP in the game directory
    let _ = fs::remove_file(game_dir.join("ERRORLOG.TXT"));
    let _ = fs::remove_file(game_dir.join("CRASH.DMP"));
}

/// Normalize CRLF to LF for cross-platform comparison.
fn normalize_crlf(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len());
    for &b in data {
        if b != b'\r' {
            out.push(b);
        }
    }
    out
}

/// Strip the `\\?\` UNC prefix that `fs::canonicalize` adds on Windows.
fn strip_unc(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        PathBuf::from(stripped)
    } else {
        p
    }
}

/// Map common NTSTATUS exception codes to human-readable names.
fn ntstatus_name(code: u32) -> &'static str {
    match code {
        0xC0000005 => "access violation",
        0xC0000374 => "heap corruption",
        0xC00000FD => "stack overflow",
        0xC0000409 => "stack buffer overrun",
        0xC000001D => "illegal instruction",
        0xC0000135 => "DLL not found",
        0xC0000142 => "DLL init failed",
        0x80000003 => "breakpoint",
        0xE06D7363 => "C++ exception", // MSVC __CxxThrowException
        _ => "exception",
    }
}

/// NTSTATUS exception codes have the high bit set, so they're negative as i32.
fn is_crash_exit_code(code: i32) -> bool {
    code < 0
}

/// Parse the exception name from the first line of ERRORLOG.TXT.
/// Expected format: "WA caused an Access Violation (0xc0000005)"
fn parse_errorlog_exception(content: Option<&str>) -> &'static str {
    if let Some(line) = content.and_then(|c| c.lines().next()) {
        // Try to extract the NTSTATUS code from parentheses
        if let Some(start) = line.find("(0x") {
            let hex_start = start + 1; // skip '('
            if let Some(end) = line[hex_start..].find(')') {
                let hex_str = &line[hex_start + 2..hex_start + end]; // skip "0x"
                if let Ok(code) = u32::from_str_radix(hex_str, 16) {
                    return ntstatus_name(code);
                }
            }
        }
    }
    "crash (SEH handled)"
}

// ─── Headful subcommand ────────────────────────────────────────────────────

const DEFAULT_HEADFUL_TIMEOUT_SECS: u64 = 150;

struct HeadfulArgs {
    filter_or_replay: Option<String>,
    no_build: bool,
    wa_path: Option<PathBuf>,
    timeout_secs: u64,
}

fn parse_headful_args(argv: &[String]) -> HeadfulArgs {
    let mut args = HeadfulArgs {
        filter_or_replay: None,
        no_build: false,
        wa_path: None,
        timeout_secs: DEFAULT_HEADFUL_TIMEOUT_SECS,
    };
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--no-build" => args.no_build = true,
            "--wa-path" => {
                i += 1;
                if i < argv.len() {
                    args.wa_path = Some(PathBuf::from(&argv[i]));
                }
            }
            "--timeout" | "-t" => {
                i += 1;
                if i < argv.len() {
                    args.timeout_secs = argv[i]
                        .parse()
                        .unwrap_or(DEFAULT_HEADFUL_TIMEOUT_SECS);
                }
            }
            s if !s.starts_with('-') && args.filter_or_replay.is_none() => {
                args.filter_or_replay = Some(s.to_string());
            }
            other => {
                eprintln!("Unknown argument: {other}");
                eprintln!(
                    "Usage: openwa-test headful [filter|replay.WAgame] [--no-build] [--wa-path PATH] [--timeout SECS]"
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }
    args
}

/// Discover replays for headful testing. Unlike headless, doesn't require _expected.log.
/// If `filter_or_replay` ends with .WAgame and exists, treat it as a direct path.
fn discover_headful_tests(filter_or_replay: Option<&str>) -> Vec<TestCase> {
    // Direct .WAgame path?
    if let Some(arg) = filter_or_replay {
        let path = Path::new(arg);
        if arg.ends_with(".WAgame") && path.exists() {
            let abs = strip_unc(fs::canonicalize(path).unwrap_or(path.to_path_buf()));
            let stem = abs
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string();
            return vec![TestCase {
                name: stem,
                replay_path: abs.clone(),
                expected_log: abs.with_extension("expected.log"),
                output_log: abs.with_extension("log"),
            }];
        }
    }

    // Scan replays dir with optional filter
    let replays_dir = Path::new(REPLAYS_DIR);
    let entries = match fs::read_dir(replays_dir) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("Cannot read {REPLAYS_DIR}: {e}");
            return Vec::new();
        }
    };

    let mut tests = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("WAgame") {
            continue;
        }
        let stem = match path.file_stem().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if let Some(filter) = filter_or_replay {
            if !stem.contains(filter) {
                continue;
            }
        }
        // No _expected.log requirement for headful
        let abs = strip_unc(fs::canonicalize(&path).unwrap_or(path.clone()));
        tests.push(TestCase {
            name: stem,
            replay_path: abs.clone(),
            expected_log: abs.with_extension("expected.log"),
            output_log: abs.with_extension("log"),
        });
    }
    tests.sort_by(|a, b| a.name.cmp(&b.name));
    tests
}

/// Analyze an OpenWA.log for panics and gameplay check markers.
struct HeadfulLogAnalysis {
    panics: Vec<String>,
    gameplay_passes: Vec<String>,
    gameplay_fails: Vec<String>,
}

fn analyze_headful_log(log_path: &Path) -> HeadfulLogAnalysis {
    let content = fs::read_to_string(log_path).unwrap_or_default();
    let mut analysis = HeadfulLogAnalysis {
        panics: Vec::new(),
        gameplay_passes: Vec::new(),
        gameplay_fails: Vec::new(),
    };
    for line in content.lines() {
        if line.contains("[PANIC]") {
            analysis.panics.push(line.to_string());
        } else if line.contains("[GAMEPLAY PASS]") {
            analysis.gameplay_passes.push(line.to_string());
        } else if line.contains("[GAMEPLAY FAIL]") {
            analysis.gameplay_fails.push(line.to_string());
        }
    }
    analysis
}

/// Run a single headful replay test.
fn run_headful_test(
    test: &TestCase,
    launcher: &Path,
    wa_exe: &Path,
    run_dir: &Path,
    timeout_secs: u64,
) -> TestResult {
    let start = Instant::now();

    let openwa_log = run_dir.join(format!("{}.openwa.log", test.name));
    let errorlog_path = run_dir.join(format!("{}.errorlog.txt", test.name));

    let result = Command::new(launcher)
        .arg("--minimized")
        .arg(wa_exe)
        .arg(&test.replay_path)
        .env("OPENWA_REPLAY_TEST", "1")
        .env("OPENWA_LOG_PATH", &openwa_log)
        .env("OPENWA_ERRORLOG_PATH", &errorlog_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();

    let duration = start.elapsed();

    match result {
        Err(e) => TestResult {
            name: test.name.clone(),
            passed: false,
            duration,
            diff_lines: Vec::new(),
            error: Some(format!("Failed to launch: {e}")),
            crashed: None,
        },
        Ok(status) => {
            let exit_code = status.code().unwrap_or(0);

            if duration >= Duration::from_secs(timeout_secs) {
                return TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration,
                    diff_lines: Vec::new(),
                    error: Some("Timeout".to_string()),
                    crashed: None,
                };
            }

            // Crash detection (same as headless)
            let errorlog_content = fs::read(&errorlog_path)
                .ok()
                .filter(|b| !b.is_empty())
                .map(|b| String::from_utf8_lossy(&b).into_owned());

            if is_crash_exit_code(exit_code) || errorlog_content.is_some() {
                let unsigned = exit_code as u32;
                let name = if is_crash_exit_code(exit_code) {
                    ntstatus_name(unsigned)
                } else {
                    parse_errorlog_exception(errorlog_content.as_deref())
                };
                return TestResult {
                    name: test.name.clone(),
                    passed: false,
                    duration,
                    diff_lines: Vec::new(),
                    error: None,
                    crashed: Some(CrashInfo {
                        exit_code: unsigned,
                        name,
                        errorlog_content,
                    }),
                };
            }

            // Analyze OpenWA.log for panics and gameplay markers
            let analysis = analyze_headful_log(&openwa_log);

            let mut detail_lines = Vec::new();
            for line in &analysis.panics {
                detail_lines.push(format!("PANIC: {line}"));
            }
            for line in &analysis.gameplay_fails {
                detail_lines.push(format!("FAIL: {line}"));
            }
            for line in &analysis.gameplay_passes {
                detail_lines.push(line.clone());
            }

            let passed = analysis.panics.is_empty()
                && analysis.gameplay_fails.is_empty()
                && !analysis.gameplay_passes.is_empty();

            let error = if analysis.gameplay_passes.is_empty()
                && analysis.panics.is_empty()
                && analysis.gameplay_fails.is_empty()
            {
                Some(format!(
                    "No gameplay markers found in log (exit code: {exit_code})"
                ))
            } else {
                None
            };

            TestResult {
                name: test.name.clone(),
                passed,
                duration,
                diff_lines: detail_lines,
                error,
                crashed: None,
            }
        }
    }
}

fn run_headful(args: HeadfulArgs) {
    let tests = discover_headful_tests(args.filter_or_replay.as_deref());
    if tests.is_empty() {
        eprintln!("No replay tests found in {REPLAYS_DIR}/");
        if let Some(f) = &args.filter_or_replay {
            eprintln!("  (filter/path: \"{f}\")");
        }
        std::process::exit(1);
    }

    // Build
    if !args.no_build {
        eprint!("Building... ");
        match build() {
            Ok(d) => eprintln!("done ({:.1}s)", d.as_secs_f64()),
            Err(e) => {
                eprintln!("FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    let wa_exe = find_wa_exe(args.wa_path.as_deref()).unwrap_or_else(|| {
        eprintln!("Cannot find WA.exe. Use --wa-path or set OPENWA_WA_PATH.");
        std::process::exit(1);
    });
    let launcher = find_launcher().unwrap_or_else(|| {
        eprintln!("Cannot find openwa-launcher.exe. Run cargo build first.");
        std::process::exit(1);
    });

    // Clean stale files from game dir
    if let Some(game_dir) = wa_exe.parent() {
        let _ = fs::remove_file(game_dir.join("ERRORLOG.TXT"));
        let _ = fs::remove_file(game_dir.join("OpenWA.log"));
    }

    let timestamp = timestamp();
    let run_dir = PathBuf::from(RUNS_DIR).join(format!("headful-{timestamp}"));
    let _ = fs::create_dir_all(&run_dir);
    let run_dir = strip_unc(fs::canonicalize(&run_dir).unwrap_or(run_dir));

    println!(
        "Running {} headful test{}...\n",
        tests.len(),
        if tests.len() == 1 { "" } else { "s" }
    );

    let wall_start = Instant::now();
    let mut results = Vec::new();

    for test in &tests {
        let result = run_headful_test(test, &launcher, &wa_exe, &run_dir, args.timeout_secs);
        print_result(&result);
        results.push(result);
    }

    let wall_time = wall_start.elapsed();

    print_summary(&results, wall_time);
    write_summary(&results, wall_time, &run_dir.join("summary.txt"));

    // Report startup check failures
    report_startup_check_failures(&results, &run_dir);

    cleanup_temp_files(&wa_exe);

    let failed = results.iter().any(|r| !r.passed);
    if failed {
        std::process::exit(1);
    }
}

// ─── Trace-desync subcommand ─────────────────────────────────────────────

struct TraceDesyncArgs {
    replay: PathBuf,
    no_build: bool,
    wa_path: Option<PathBuf>,
}

fn parse_trace_desync_args(argv: &[String]) -> TraceDesyncArgs {
    let mut replay = None;
    let mut no_build = false;
    let mut wa_path = None;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--no-build" => no_build = true,
            "--wa-path" => {
                i += 1;
                if i < argv.len() {
                    wa_path = Some(PathBuf::from(&argv[i]));
                }
            }
            s if !s.starts_with('-') => replay = Some(PathBuf::from(s)),
            other => {
                eprintln!("Unknown argument: {other}");
                eprintln!(
                    "Usage: openwa-test trace-desync <replay.WAgame> [--no-build] [--wa-path PATH]"
                );
                std::process::exit(1);
            }
        }
        i += 1;
    }
    let replay = replay.unwrap_or_else(|| {
        eprintln!("Usage: openwa-test trace-desync <replay.WAgame> [--no-build] [--wa-path PATH]");
        std::process::exit(1);
    });
    if replay.extension().and_then(|e| e.to_str()) != Some("WAgame") {
        eprintln!(
            "ERROR: Expected a .WAgame replay file, got: {}",
            replay.display()
        );
        std::process::exit(1);
    }
    if !replay.exists() {
        eprintln!("ERROR: Replay file not found: {}", replay.display());
        std::process::exit(1);
    }
    TraceDesyncArgs {
        replay,
        no_build,
        wa_path,
    }
}

struct FrameHash {
    frame: u32,
    checksum_a: u32,
    checksum_b: u32,
}

fn read_hash_log(path: &Path) -> Vec<FrameHash> {
    let content = fs::read_to_string(path).unwrap_or_default();
    content
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 3 {
                Some(FrameHash {
                    frame: parts[0].parse().ok()?,
                    checksum_a: u32::from_str_radix(parts[1], 16).ok()?,
                    checksum_b: u32::from_str_radix(parts[2], 16).ok()?,
                })
            } else {
                None
            }
        })
        .collect()
}

fn run_trace_instance(
    launcher: &Path,
    wa_exe: &Path,
    replay: &Path,
    hash_path: &Path,
    openwa_log: &Path,
    errorlog_path: &Path,
    is_baseline: bool,
) -> (Duration, bool) {
    let start = Instant::now();
    let mut cmd = Command::new(launcher);
    cmd.arg(wa_exe)
        .arg("/getlog")
        .arg(replay)
        .env("OPENWA_HEADLESS", "1")
        .env("OPENWA_TRACE_DESYNC", "1")
        .env("OPENWA_TRACE_HASH_PATH", hash_path)
        .env("OPENWA_LOG_PATH", openwa_log)
        .env("OPENWA_ERRORLOG_PATH", errorlog_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW);

    if is_baseline {
        cmd.env("OPENWA_TRACE_BASELINE", "1");
    }

    let status = cmd.status();
    let duration = start.elapsed();
    let ok = match status {
        Ok(s) => {
            if !s.success() {
                let code = s.code().unwrap_or(-1);
                eprintln!(
                    "  WARNING: WA.exe exited with code {} (0x{:08X})",
                    code, code as u32
                );
                false
            } else {
                true
            }
        }
        Err(e) => {
            eprintln!("  ERROR: Failed to launch: {e}");
            false
        }
    };
    (duration, ok)
}

fn compare_hashes(baseline: &[FrameHash], hooks: &[FrameHash]) {
    let min_len = baseline.len().min(hooks.len());
    if min_len == 0 {
        eprintln!(
            "ERROR: No frame hashes captured (baseline: {}, hooks: {})",
            baseline.len(),
            hooks.len()
        );
        std::process::exit(1);
    }

    println!("Comparing {} frames...\n", min_len);

    let mut first_divergence = None;
    for i in 0..min_len {
        if baseline[i].checksum_a != hooks[i].checksum_a
            || baseline[i].checksum_b != hooks[i].checksum_b
        {
            first_divergence = Some(i);
            break;
        }
    }

    match first_divergence {
        None => {
            if baseline.len() != hooks.len() {
                println!(
                    "WARN: Frame count differs (baseline: {}, hooks: {}) \
                     but all {} common frames match.",
                    baseline.len(),
                    hooks.len(),
                    min_len
                );
            } else {
                println!(
                    "{}",
                    green(&format!(
                        "OK: All {} frames have identical checksums.",
                        min_len
                    ))
                );
            }
        }
        Some(idx) => {
            let b = &baseline[idx];
            let h = &hooks[idx];
            println!("{}", red(&format!("DESYNC at frame {}!", b.frame)));
            println!("  baseline: A={:08X} B={:08X}", b.checksum_a, b.checksum_b);
            println!("  hooks:    A={:08X} B={:08X}", h.checksum_a, h.checksum_b);
            if idx > 0 {
                let prev = &baseline[idx - 1];
                println!(
                    "  last matching frame: {} (A={:08X})",
                    prev.frame, prev.checksum_a
                );
            }
            let divergent = (idx..min_len)
                .filter(|&i| {
                    baseline[i].checksum_a != hooks[i].checksum_a
                        || baseline[i].checksum_b != hooks[i].checksum_b
                })
                .count();
            println!(
                "  {} of {} remaining frames diverge",
                divergent,
                min_len - idx
            );
        }
    }
}

fn run_trace_desync(args: TraceDesyncArgs) {
    // Build
    if !args.no_build {
        eprint!("Building... ");
        match build() {
            Ok(d) => eprintln!("done ({:.1}s)", d.as_secs_f64()),
            Err(e) => {
                eprintln!("FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Find executables
    let wa_exe = find_wa_exe(args.wa_path.as_deref()).unwrap_or_else(|| {
        eprintln!("Cannot find WA.exe. Use --wa-path or set OPENWA_WA_PATH.");
        std::process::exit(1);
    });
    let launcher = find_launcher().unwrap_or_else(|| {
        eprintln!("Cannot find openwa-launcher.exe. Run cargo build first.");
        std::process::exit(1);
    });

    // Resolve replay path
    let replay = strip_unc(fs::canonicalize(&args.replay).unwrap_or(args.replay));

    // Create run directory
    let ts = timestamp();
    let run_dir = PathBuf::from(RUNS_DIR).join(format!("trace-{ts}"));
    let _ = fs::create_dir_all(&run_dir);
    let run_dir = strip_unc(fs::canonicalize(&run_dir).unwrap_or(run_dir));

    // Run baseline (minimal hooks)
    eprint!("Running baseline (minimal hooks)... ");
    let baseline_hash = run_dir.join("baseline_hashes.log");
    let baseline_log = run_dir.join("baseline_openwa.log");
    let baseline_errlog = run_dir.join("baseline_errorlog.txt");
    let (baseline_dur, _) = run_trace_instance(
        &launcher,
        &wa_exe,
        &replay,
        &baseline_hash,
        &baseline_log,
        &baseline_errlog,
        true,
    );
    let baseline_hashes = read_hash_log(&baseline_hash);
    eprintln!(
        "done ({:.1}s, {} frames)",
        baseline_dur.as_secs_f64(),
        baseline_hashes.len()
    );

    // Run with all hooks
    eprint!("Running with all hooks...            ");
    let hooks_hash = run_dir.join("hooks_hashes.log");
    let hooks_log = run_dir.join("hooks_openwa.log");
    let hooks_errlog = run_dir.join("hooks_errorlog.txt");
    let (hooks_dur, _) = run_trace_instance(
        &launcher,
        &wa_exe,
        &replay,
        &hooks_hash,
        &hooks_log,
        &hooks_errlog,
        false,
    );
    let hooks_hashes = read_hash_log(&hooks_hash);
    eprintln!(
        "done ({:.1}s, {} frames)",
        hooks_dur.as_secs_f64(),
        hooks_hashes.len()
    );

    println!();
    compare_hashes(&baseline_hashes, &hooks_hashes);

    // Clean up
    cleanup_temp_files(&wa_exe);
    println!("\nRun directory: {}", run_dir.display());
}

// ─── Startup check reporting ─────────────────────────────────────────────────

/// Scan the first test's OpenWA.log for `[CHECK FAIL]` lines and report them
/// once. All test instances load the same DLL against the same WA.exe, so the
/// startup check results are deterministic — checking one log is sufficient.
fn report_startup_check_failures(results: &[TestResult], run_dir: &Path) {
    let first = match results.first() {
        Some(r) => r,
        None => return,
    };
    let log_path = run_dir.join(format!("{}.openwa.log", first.name));
    let content = match fs::read_to_string(&log_path) {
        Ok(c) => c,
        Err(_) => return,
    };

    let failures: Vec<&str> = content
        .lines()
        .filter(|l| l.contains("[CHECK FAIL]"))
        .collect();

    if failures.is_empty() {
        return;
    }

    println!();
    let msg = format!(
        "Startup check failures detected (from {}.openwa.log):",
        first.name
    );
    if use_color() {
        println!("\x1b[33m{msg}\x1b[0m");
    } else {
        println!("{msg}");
    }
    for line in &failures {
        println!("  {line}");
    }
}

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
    // Check for subcommands before normal arg parsing
    let argv: Vec<String> = env::args().skip(1).collect();
    match argv.first().map(|s| s.as_str()) {
        Some("headful") => {
            let sub_args = parse_headful_args(&argv[1..]);
            run_headful(sub_args);
            return;
        }
        Some("trace-desync") => {
            let sub_args = parse_trace_desync_args(&argv[1..]);
            run_trace_desync(sub_args);
            return;
        }
        _ => {}
    }

    let args = parse_args();

    // Discover tests
    let tests = discover_tests(args.filter.as_deref());
    if tests.is_empty() {
        eprintln!("No replay tests found in {REPLAYS_DIR}/");
        if let Some(f) = &args.filter {
            eprintln!("  (filter: \"{f}\")");
        }
        std::process::exit(1);
    }

    // Build
    if !args.no_build {
        eprint!("Building... ");
        match build() {
            Ok(d) => eprintln!("done ({:.1}s)", d.as_secs_f64()),
            Err(e) => {
                eprintln!("FAILED: {e}");
                std::process::exit(1);
            }
        }
    }

    // Find executables
    let wa_exe = find_wa_exe(args.wa_path.as_deref()).unwrap_or_else(|| {
        eprintln!("Cannot find WA.exe. Use --wa-path or set OPENWA_WA_PATH.");
        std::process::exit(1);
    });
    let launcher = find_launcher().unwrap_or_else(|| {
        eprintln!("Cannot find openwa-launcher.exe. Run cargo build first.");
        std::process::exit(1);
    });

    // Create run directory
    let timestamp = timestamp();
    let run_dir = PathBuf::from(RUNS_DIR).join(&timestamp);
    let _ = fs::create_dir_all(&run_dir);
    let run_dir = strip_unc(fs::canonicalize(&run_dir).unwrap_or(run_dir));

    let jobs = args.jobs.max(1);
    println!(
        "Running {} test{} ({} concurrent)...\n",
        tests.len(),
        if tests.len() == 1 { "" } else { "s" },
        jobs
    );

    // Run
    let wall_start = Instant::now();
    let results = run_tests_parallel(tests, jobs, &launcher, &wa_exe, &run_dir);
    let wall_time = wall_start.elapsed();

    // Summary
    print_summary(&results, wall_time);
    write_summary(&results, wall_time, &run_dir.join("summary.txt"));

    // Report startup check failures from the DLL (once, from the first test's log)
    report_startup_check_failures(&results, &run_dir);

    // Clean up per-PID temp files from the game directory
    cleanup_temp_files(&wa_exe);

    let failed = results.iter().any(|r| !r.passed);
    if failed {
        std::process::exit(1);
    }
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d_%H-%M-%S").to_string()
}
