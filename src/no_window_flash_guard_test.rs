//! Structural guard against a real regression class: a GUI-subsystem app
//! (`brute-desktop.exe`) spawning a console-subsystem child process
//! (`llama-cli.exe`, `nvidia-smi.exe`, `vulkaninfo.exe`) without
//! `CREATE_NO_WINDOW` makes Windows flash a visible console window on
//! every single launch - hit for real in `runtime::process` and
//! `hardware::gpu` (see `docs/known-limitations.md`). This scans every
//! source file for `Command::new(` and requires the same file to also
//! call `configure_no_window`, so a new Windows-reachable process launch
//! added later can't silently reintroduce the flash.

use std::fs;
use std::path::Path;

/// Files that spawn a process but are exempt: either the code path only
/// ever compiles/runs on a platform where a flashing console window is
/// not a concept (macOS/Linux), or the `Command::new` call is test-only
/// scaffolding that never runs in a shipped build.
const EXEMPT_FILES: &[&str] = &[
    "platform/macos.rs",
    "platform/linux.rs",
    "library/scan.rs", // test-only `mklink` junction creation, see its own comment
    "runtime/process.rs", // defines configure_no_window itself
];

#[test]
fn every_command_new_call_site_configures_no_window() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    scan_dir(&src_dir, &mut offenders);
    assert!(
        offenders.is_empty(),
        "these files spawn a process via Command::new without calling \
         configure_no_window (or being explicitly exempted in \
         no_window_flash_guard_test.rs): {offenders:?}"
    );
}

fn scan_dir(dir: &Path, offenders: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("readable src directory") {
        let entry = entry.expect("readable directory entry");
        let path = entry.path();
        if path.is_dir() {
            scan_dir(&path, offenders);
            continue;
        }
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let relative = path
            .strip_prefix(Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
            .expect("path under src/")
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
            .to_string();
        if EXEMPT_FILES.iter().any(|exempt| relative == *exempt) {
            continue;
        }
        let contents = fs::read_to_string(&path).expect("readable source file");
        if contents.contains("Command::new(") && !contents.contains("configure_no_window") {
            offenders.push(relative);
        }
    }
}
