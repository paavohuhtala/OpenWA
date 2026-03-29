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

/// Known WA.exe locations to try if not specified.
const WA_CANDIDATES: &[&str] = &[
    "I:/games/SteamLibrary/steamapps/common/Worms Armageddon/WA.exe",
    "C:/Program Files (x86)/Steam/steamapps/common/Worms Armageddon/WA.exe",
];

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

    if let Ok(p) = env::var("OPENWA_WA_PATH") {
        let path = PathBuf::from(p);
        if path.exists() {
            return Some(path);
        }
    }

    for candidate in WA_CANDIDATES {
        let path = PathBuf::from(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
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
                    error: Some(format!(
                        "No output log generated (exit code: {exit_code})"
                    )),
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

// ─── Main ───────────────────────────────────────────────────────────────────

fn main() {
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
