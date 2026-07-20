fn main() {
    // `tauri_build::build()` only re-runs on changes to `tauri.conf.json`
    // itself, not on the *content* of the icon files it references - a
    // real staleness bug hit during verification: replacing
    // `icons/icon.ico` in place (same path, new bytes) left the
    // Windows-resource-embedded icon on the previously built `.exe`
    // stale until something else forced a rebuild. These explicit
    // declarations make every icon regeneration (see
    // `scripts/generate-icons.ps1`) reliably trigger a rebuild.
    for entry in std::fs::read_dir("icons").expect("icons/ directory must exist") {
        let path = entry.expect("readable icons/ directory entry").path();
        if path.is_file() {
            println!("cargo:rerun-if-changed={}", path.display());
        }
    }
    tauri_build::build()
}
