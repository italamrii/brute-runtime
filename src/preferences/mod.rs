//! Local-only user preference profile that feeds the recommendation
//! engine (`recommend`). One file, `%LOCALAPPDATA%\BruteRuntime\
//! preferences.json`, never git-tracked, never uploaded, never synced -
//! there is no account and no cloud sync for this or anything else in
//! BRUTE. See `docs/privacy-model.md`.
//!
//! Split into a small set of simple, user-facing choices (`language`,
//! `use_case`, `priority` - the only three most users should ever need to
//! touch) and a larger set of advanced constraints that stay hidden
//! behind an "Advanced" toggle in the UI but are just as real: memory/
//! download-size ceilings, license/commercial-use requirements, and
//! family allow/exclude lists. Every advanced field defaults to "no
//! constraint" (empty list / `None` / `false`), never a guessed limit.

use crate::errors::PreferencesError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const PREFERENCES_SCHEMA_VERSION: &str = "preferences-v1";

/// The three simple, user-facing choices Stage B.3 calls for. Deliberately
/// a small, closed set - not the catalog's full `TaskCategory` vocabulary
/// (which has 14 variants for internal matching) or `recommend::Priority`
/// (8 variants tuned for the ranking formula). Step 5 (recommendation
/// engine v2) maps these onto that richer internal vocabulary; this
/// module only owns what the user actually chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LanguagePreference {
    Arabic,
    English,
    #[default]
    Both,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum UseCase {
    #[default]
    GeneralAssistant,
    Coding,
    Documents,
    Writing,
    Summarization,
    Reasoning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SpeedQualityPriority {
    Fastest,
    #[default]
    Balanced,
    BestQuality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GpuPreference {
    #[default]
    NoPreference,
    PreferGpu,
    RequireGpu,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preferences {
    pub schema_version: String,

    // ---- Simple, always-visible choices --------------------------------
    pub language: LanguagePreference,
    pub use_case: UseCase,
    pub priority: SpeedQualityPriority,

    // ---- Advanced, hidden-by-default constraints -----------------------
    /// Boost Arabic-capable models even when `language` is `Both` -
    /// distinct from `language` itself so a bilingual user can still say
    /// "but weigh Arabic quality more heavily."
    pub arabic_priority: bool,
    pub english_priority: bool,
    /// Prefer smaller/lower-memory models even at some quality cost.
    pub memory_conservative_mode: bool,
    pub cpu_only: bool,
    pub gpu_preference: GpuPreference,
    /// Hides/disables anything that would suggest a network action
    /// (Discover's Download flow, "View official source") - independent
    /// of the fact that model inference itself is always offline
    /// regardless of this setting.
    pub offline_only: bool,
    /// Empty = no restriction. When non-empty, only these license
    /// identifiers (matched against `ModelBuild::license`'s `identifier`,
    /// e.g. `"apache-2.0"`) are considered acceptable.
    pub permitted_licenses: Vec<String>,
    pub commercial_use_required: bool,
    /// `family_id` values (see `catalog::schema::ModelBuild::family_id`).
    pub preferred_families: Vec<String>,
    pub excluded_families: Vec<String>,
    pub max_download_size_bytes: Option<u64>,
    pub max_ram_bytes: Option<u64>,
    pub max_vram_bytes: Option<u64>,

    pub updated_at_rfc3339: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            schema_version: PREFERENCES_SCHEMA_VERSION.to_string(),
            language: LanguagePreference::default(),
            use_case: UseCase::default(),
            priority: SpeedQualityPriority::default(),
            arabic_priority: false,
            english_priority: false,
            memory_conservative_mode: false,
            cpu_only: false,
            gpu_preference: GpuPreference::default(),
            offline_only: false,
            permitted_licenses: Vec::new(),
            commercial_use_required: false,
            preferred_families: Vec::new(),
            excluded_families: Vec::new(),
            max_download_size_bytes: None,
            max_ram_bytes: None,
            max_vram_bytes: None,
            updated_at_rfc3339: String::new(),
        }
    }
}

/// Hard ceiling on how many family/license entries an advanced list may
/// hold - defends against a corrupted or hand-edited file turning every
/// downstream recommendation pass into an unbounded scan.
const MAX_LIST_ENTRIES: usize = 200;

impl Preferences {
    /// Basic sanity checks, run on every load - a preferences file is
    /// locally generated (not adversarial like a catalog file downloaded
    /// from elsewhere), but a hand-edited or corrupted one should still
    /// fail loudly rather than silently misbehave downstream.
    pub fn validate(&self) -> Result<(), String> {
        if self.permitted_licenses.len() > MAX_LIST_ENTRIES {
            return Err(format!(
                "permitted_licenses has {} entries, exceeding the {MAX_LIST_ENTRIES} limit",
                self.permitted_licenses.len()
            ));
        }
        if self.preferred_families.len() > MAX_LIST_ENTRIES {
            return Err(format!(
                "preferred_families has {} entries, exceeding the {MAX_LIST_ENTRIES} limit",
                self.preferred_families.len()
            ));
        }
        if self.excluded_families.len() > MAX_LIST_ENTRIES {
            return Err(format!(
                "excluded_families has {} entries, exceeding the {MAX_LIST_ENTRIES} limit",
                self.excluded_families.len()
            ));
        }
        let overlap: Vec<&String> = self
            .preferred_families
            .iter()
            .filter(|f| self.excluded_families.contains(f))
            .collect();
        if !overlap.is_empty() {
            return Err(format!(
                "the same family appears in both preferred_families and excluded_families: {overlap:?}"
            ));
        }
        Ok(())
    }
}

/// `%LOCALAPPDATA%\BruteRuntime\preferences.json` - see
/// `identity::default_local_state_dir` for the parent.
pub fn default_preferences_path() -> PathBuf {
    crate::identity::default_local_state_dir().join("preferences.json")
}

/// A brand-new install has no preferences file yet - that is not an
/// error, it just means every default applies (`Preferences::default()`).
pub fn load_preferences_from(path: &Path) -> Result<Preferences, PreferencesError> {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Preferences::default());
        }
        Err(source) => {
            return Err(PreferencesError::Read {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let prefs: Preferences =
        serde_json::from_str(&contents).map_err(|source| PreferencesError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    if let Err(reason) = prefs.validate() {
        return Err(PreferencesError::Parse {
            path: path.to_path_buf(),
            source: serde::de::Error::custom(reason),
        });
    }
    Ok(prefs)
}

/// Writes via write-to-temp-then-rename, matching `conversations`' atomic
/// pattern - the same reasoning applies (a crash mid-write must never
/// leave a half-written preferences file that then fails every future
/// load).
pub fn save_preferences_to(path: &Path, prefs: &Preferences) -> Result<(), PreferencesError> {
    prefs.validate().map_err(|reason| PreferencesError::Write {
        path: path.to_path_buf(),
        source: std::io::Error::other(reason),
    })?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| PreferencesError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let json = serde_json::to_string_pretty(prefs).expect("Preferences always serializes");
    let tmp_path = {
        let mut p = path.as_os_str().to_owned();
        p.push(".tmp");
        PathBuf::from(p)
    };
    std::fs::write(&tmp_path, &json).map_err(|source| PreferencesError::Write {
        path: tmp_path.clone(),
        source,
    })?;
    std::fs::rename(&tmp_path, path).map_err(|source| PreferencesError::Write {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "brute-preferences-test-{name}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn loading_a_missing_file_returns_defaults_not_an_error() {
        let dir = tmp_dir("missing");
        let prefs = load_preferences_from(&dir.join("preferences.json")).unwrap();
        assert_eq!(prefs.language, LanguagePreference::Both);
        assert_eq!(prefs.use_case, UseCase::GeneralAssistant);
        assert_eq!(prefs.priority, SpeedQualityPriority::Balanced);
        assert!(prefs.permitted_licenses.is_empty());
        assert!(prefs.max_ram_bytes.is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn save_and_load_roundtrip_preserves_every_field() {
        let dir = tmp_dir("roundtrip");
        let path = dir.join("preferences.json");
        let mut prefs = Preferences {
            language: LanguagePreference::Arabic,
            use_case: UseCase::Coding,
            priority: SpeedQualityPriority::Fastest,
            arabic_priority: true,
            cpu_only: true,
            gpu_preference: GpuPreference::RequireGpu,
            offline_only: true,
            permitted_licenses: vec!["apache-2.0".to_string(), "mit".to_string()],
            commercial_use_required: true,
            preferred_families: vec!["qwen2.5".to_string()],
            excluded_families: vec!["llama-3.1-70b-instruct".to_string()],
            max_download_size_bytes: Some(5_000_000_000),
            max_ram_bytes: Some(8_000_000_000),
            max_vram_bytes: Some(4_000_000_000),
            updated_at_rfc3339: "2026-07-21T00:00:00Z".to_string(),
            ..Default::default()
        };
        prefs.updated_at_rfc3339 = "2026-07-21T00:00:00Z".to_string();
        save_preferences_to(&path, &prefs).unwrap();

        let loaded = load_preferences_from(&path).unwrap();
        assert_eq!(loaded.language, LanguagePreference::Arabic);
        assert_eq!(loaded.use_case, UseCase::Coding);
        assert_eq!(loaded.priority, SpeedQualityPriority::Fastest);
        assert!(loaded.arabic_priority);
        assert!(loaded.cpu_only);
        assert_eq!(loaded.gpu_preference, GpuPreference::RequireGpu);
        assert!(loaded.offline_only);
        assert_eq!(loaded.permitted_licenses, vec!["apache-2.0", "mit"]);
        assert!(loaded.commercial_use_required);
        assert_eq!(loaded.preferred_families, vec!["qwen2.5"]);
        assert_eq!(loaded.max_download_size_bytes, Some(5_000_000_000));
        assert_eq!(loaded.max_ram_bytes, Some(8_000_000_000));
        assert_eq!(loaded.max_vram_bytes, Some(4_000_000_000));

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn atomic_write_leaves_no_tmp_file_behind_on_success() {
        let dir = tmp_dir("atomic");
        let path = dir.join("preferences.json");
        save_preferences_to(&path, &Preferences::default()).unwrap();

        assert!(path.is_file());
        assert!(!dir.join("preferences.json.tmp").exists());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn rejects_a_family_listed_as_both_preferred_and_excluded() {
        let dir = tmp_dir("overlap");
        let path = dir.join("preferences.json");
        let prefs = Preferences {
            preferred_families: vec!["qwen2.5".to_string()],
            excluded_families: vec!["qwen2.5".to_string()],
            ..Default::default()
        };
        let result = save_preferences_to(&path, &prefs);
        assert!(result.is_err());
        assert!(
            !path.exists(),
            "an invalid preferences file must never be written"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_corrupt_preferences_file_is_a_real_error_not_a_silent_default() {
        let dir = tmp_dir("corrupt");
        let path = dir.join("preferences.json");
        std::fs::write(&path, "{ not valid json at all").unwrap();

        let result = load_preferences_from(&path);
        assert!(result.is_err());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn defaults_never_constrain_anything() {
        // The mission's own rule: advanced constraints default to "no
        // constraint," never a guessed limit.
        let prefs = Preferences::default();
        assert!(!prefs.cpu_only);
        assert!(!prefs.offline_only);
        assert!(!prefs.commercial_use_required);
        assert!(prefs.permitted_licenses.is_empty());
        assert!(prefs.preferred_families.is_empty());
        assert!(prefs.excluded_families.is_empty());
        assert!(prefs.max_download_size_bytes.is_none());
        assert!(prefs.max_ram_bytes.is_none());
        assert!(prefs.max_vram_bytes.is_none());
        assert_eq!(prefs.gpu_preference, GpuPreference::NoPreference);
    }
}
