//! Shared confidence/provenance primitive for Stage 1 (hardware profile,
//! catalog, estimator, fit, recommendation, calibration).
//!
//! Deliberately separate from `hardware::Confidence` (Stage 0). Stage 0's
//! four-way Measured/Detected/Inferred/Unavailable model does not have a
//! slot for "this number came from curated catalog metadata we did not
//! measure or infer at all" - conflating that into `Inferred` would blur a
//! distinction the Stage 1 product principle requires every reader to be
//! able to see. Keeping the types separate also means Stage 1 code can
//! never accidentally destabilize a Stage 0 struct's serialized shape.

use serde::Serialize;

/// How sure we are about a value, and where it came from.
///
/// - `Measured`: read directly from an OS API or CPU instruction on *this*
///   machine, no interpretation (mirrors `hardware::Confidence::Measured`).
/// - `Detected`: from an external tool or indirect signal on *this*
///   machine that we trust but don't control.
/// - `Inferred`: derived via a documented formula/heuristic from other
///   facts - not observed directly. Every `Inferred` value in Stage 1
///   carries a `note` naming the formula/assumption used.
/// - `Catalog`: taken verbatim from curated model-build metadata (a fact
///   about the *model*, not about this machine) - never blended with
///   Measured/Detected/Inferred confidence about the machine itself.
/// - `Unknown`: no trustworthy source exists for this value. Never
///   defaulted to a plausible-looking number, and never collapsed into a
///   negative judgement (e.g. `Unknown` fit is not `NotRecommended`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Provenance {
    Measured,
    Detected,
    Inferred,
    Catalog,
    Unknown,
}

/// A value paired with its provenance and a human-readable note (the exact
/// API/tool/formula/catalog-field it came from, or why it's unknown).
#[derive(Debug, Clone, Serialize)]
pub struct Valued<T> {
    pub value: Option<T>,
    pub provenance: Provenance,
    pub note: String,
}

impl<T> Valued<T> {
    /// `Measured`/`Detected`/`Unknown` constructors are deliberately not
    /// provided here: every current caller reaches those provenances via
    /// `From<HardwareField<T>>` below (Stage 0's detectors are the only
    /// source of Measured/Detected/Unknown machine facts in this codebase
    /// so far). `Inferred` and `Catalog` are Stage 1's own new provenance
    /// kinds (formulas and curated model metadata respectively), so they
    /// get direct constructors here.
    pub fn inferred(value: T, note: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            provenance: Provenance::Inferred,
            note: note.into(),
        }
    }

    pub fn catalog(value: T, note: impl Into<String>) -> Self {
        Self {
            value: Some(value),
            provenance: Provenance::Catalog,
            note: note.into(),
        }
    }
}

/// Converts a Stage 0 `hardware::HardwareField<T>` into a Stage 1
/// `Valued<T>` for reuse inside the hardware capability profile, mapping
/// `Unavailable` -> `Unknown` (same meaning, Stage 1's vocabulary).
impl<T> From<crate::hardware::HardwareField<T>> for Valued<T> {
    fn from(field: crate::hardware::HardwareField<T>) -> Self {
        use crate::hardware::Confidence as C;
        let provenance = match field.confidence {
            C::Measured => Provenance::Measured,
            C::Detected => Provenance::Detected,
            C::Inferred => Provenance::Inferred,
            C::Unavailable => Provenance::Unknown,
        };
        Self {
            value: field.value,
            provenance,
            note: field.source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::HardwareField;

    #[test]
    fn hardware_field_conversion_maps_unavailable_to_unknown() {
        let field: HardwareField<u64> = HardwareField::unavailable("api failed");
        let v: Valued<u64> = field.into();
        assert_eq!(v.provenance, Provenance::Unknown);
        assert_eq!(v.note, "api failed");
    }

    #[test]
    fn hardware_field_conversion_preserves_measured() {
        let field = HardwareField::measured(42u64, "test api");
        let v: Valued<u64> = field.into();
        assert_eq!(v.provenance, Provenance::Measured);
        assert_eq!(v.value, Some(42));
    }
}
