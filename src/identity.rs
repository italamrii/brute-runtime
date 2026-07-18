//! Local instance identity — Stage 2's privacy-review fix.
//!
//! Stage 1's `profile::compute_machine_id` is a coarse hash of CPU brand,
//! core counts, rounded RAM, and OS build. It's not a hardware serial
//! number, but it *is* fully deterministic from hardware alone, so it
//! never changes across a Windows reinstall on the same physical machine
//! — a real, if low-entropy, stable cross-install fingerprint. Stage 2
//! needs local identifiers that are genuinely random and resettable, for
//! anything that gets saved locally (runtime tuning profiles) or could
//! appear in a shareable export.
//!
//! This module generates a random 128-bit ID via `BCryptGenRandom` (the
//! Windows CNG system RNG — a direct OS call, not a bundled PRNG crate),
//! persists it to a local, per-user file outside the repository, and
//! reuses it on subsequent runs until explicitly reset. Deleting the file
//! (or running `brute privacy reset-id`) generates a brand new one.
//!
//! `local_instance_id` is safe to include in shareable exports: it
//! identifies nothing about the hardware, carries no cross-machine
//! meaning (two runs on two different machines are equally likely to
//! collide as two runs on the same one), and the user can reset it at
//! will. See `docs/privacy-model.md`.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use windows::Win32::Security::Cryptography::{BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom};

/// `%LOCALAPPDATA%\BruteRuntime\instance-id` — per-user local state, never
/// part of the git-tracked repository (unlike the curated catalog/
/// calibration seed data under `data/`, this is machine-local and
/// user-specific by nature).
pub fn default_instance_id_path() -> PathBuf {
    default_local_state_dir().join("instance-id")
}

/// `%LOCALAPPDATA%\BruteRuntime\` — the root for all Stage 2 local state
/// (instance ID, saved runtime profiles, tuning progress/cancel files).
pub fn default_local_state_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("BruteRuntime")
}

/// Loads the persisted local instance ID, generating and saving a new
/// random one on first use.
pub fn load_or_create_local_instance_id(path: &Path) -> io::Result<String> {
    if let Ok(existing) = fs::read_to_string(path) {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    let id = generate_random_id();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, &id)?;
    Ok(id)
}

/// Deletes the persisted ID so the next `load_or_create_local_instance_id`
/// call generates a fresh, unrelated one. Not an error if it never existed.
pub fn reset_local_instance_id(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

/// Generates a random, non-identifying local ID (128 bits of OS entropy,
/// hex-encoded). Used both for the local instance ID above and for
/// runtime profile IDs (`tuning::runtime_profile`) - anywhere Stage 2
/// needs a label that is unique and resettable but carries no hardware
/// or user meaning.
pub fn generate_random_id() -> String {
    let mut bytes = [0u8; 16];
    // BCRYPT_USE_SYSTEM_PREFERRED_RNG lets the system choose its default
    // RNG algorithm without us opening an explicit algorithm provider
    // handle - the standard lightweight way to ask Windows for random
    // bytes. Falls back to a coarse time-based value only if the OS call
    // itself fails, which should not normally happen on any supported
    // Windows version - this is not a cryptographic secret, just a local
    // non-identifying tag, so that fallback is an acceptable last resort
    // rather than a hard failure of the whole CLI.
    let status = unsafe { BCryptGenRandom(None, &mut bytes, BCRYPT_USE_SYSTEM_PREFERRED_RNG) };
    if status.0 != 0 {
        let fallback: u128 = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        bytes = fallback.to_le_bytes();
    }
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    format!("instance-{hex}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_path(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-identity-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn generates_a_well_formed_random_id() {
        let id = generate_random_id();
        assert!(id.starts_with("instance-"));
        assert_eq!(id.len(), "instance-".len() + 32);
    }

    #[test]
    fn two_generated_ids_are_different() {
        assert_ne!(generate_random_id(), generate_random_id());
    }

    #[test]
    fn load_or_create_persists_and_reuses_the_same_id() {
        let path = tmp_path("id1");
        let first = load_or_create_local_instance_id(&path).unwrap();
        let second = load_or_create_local_instance_id(&path).unwrap();
        assert_eq!(first, second);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn reset_causes_a_brand_new_id_to_be_generated() {
        let path = tmp_path("id2");
        let first = load_or_create_local_instance_id(&path).unwrap();
        reset_local_instance_id(&path).unwrap();
        let second = load_or_create_local_instance_id(&path).unwrap();
        assert_ne!(first, second);
        fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    #[test]
    fn resetting_a_never_created_id_is_not_an_error() {
        let path = tmp_path("never-existed");
        assert!(reset_local_instance_id(&path).is_ok());
    }
}
