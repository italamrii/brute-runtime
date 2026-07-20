//! Same structural guard as `brute`'s `no_window_flash_guard_test.rs`,
//! applied to this crate: a GUI-subsystem app spawning a console-
//! subsystem child process without `CREATE_NO_WINDOW` flashes a visible
//! console window on Windows. `brute-desktop` currently launches no
//! processes directly at all - every subprocess call goes through
//! `brute::runtime::process`, which already applies the flag - but this
//! guard exists so a future command added directly to this crate can't
//! silently reintroduce the bug.

use std::fs;
use std::path::Path;

#[test]
fn every_command_new_call_site_configures_no_window() {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders = Vec::new();
    scan_dir(&src_dir, &mut offenders);
    assert!(
        offenders.is_empty(),
        "these files spawn a process via Command::new without calling \
         a *_no_window helper: {offenders:?} - route the launch through \
         brute::runtime::process (preferred) or apply \
         brute::runtime::process::configure_no_window directly"
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
        let contents = fs::read_to_string(&path).expect("readable source file");
        if contents.contains("Command::new(") && !contents.contains("configure_no_window") {
            offenders.push(path.display().to_string());
        }
    }
}
