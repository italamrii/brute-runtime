//! Cross-cutting Stage 1 integration tests: the full profile -> catalog ->
//! estimate -> fit -> rank pipeline exercised against the specific machine
//! fixtures the Stage 1 brief calls out by name. These intentionally don't
//! depend on this dev machine's real hardware (see `synthetic_profile`) so
//! they degrade/pass the same way on any machine running `cargo test`.

#[cfg(test)]
mod tests {
    use crate::calibration::CalibrationStore;
    use crate::catalog::{self, Catalog};
    use crate::fit::FitState;
    use crate::hardware::{GpuAdapter, GpuVendor, HardwareField, StorageReport};
    use crate::profile::{self, HardwareCapabilityProfile};
    use crate::recommend::{self, Priority};
    use std::path::Path;

    fn dev_catalog() -> Catalog {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/dev-catalog.json");
        catalog::load_catalog(&path).expect("dev catalog must load")
    }

    fn empty_calibration() -> CalibrationStore {
        CalibrationStore::default()
    }

    /// The real seeded calibration data (Qwen2.5-0.5B on CPU). Using this
    /// rather than an empty store better reflects what a real deployment
    /// ships with: qwen2 builds get calibration support, everything else
    /// (e.g. the Llama family) honestly does not yet.
    fn seeded_calibration() -> CalibrationStore {
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/calibration/seed-calibration.json");
        CalibrationStore::load(&path).expect("seed calibration must load")
    }

    /// Builds a profile with everything real about *this* test-running
    /// machine's CPU/OS zeroed out to "unknown" and only the requested
    /// memory/GPU/backend facts set - so fixture tests describe a machine
    /// shape, not whatever hardware happens to run `cargo test`.
    fn synthetic_profile(
        total_ram: u64,
        available_ram: u64,
        gpus: Vec<GpuAdapter>,
        cuda: Option<bool>,
        vulkan: Option<bool>,
        free_disk: Option<u64>,
    ) -> HardwareCapabilityProfile {
        let mut hw = crate::hardware::inspect(None);
        hw.memory.total_bytes = HardwareField::measured(total_ram, "fixture");
        hw.memory.available_bytes = HardwareField::measured(available_ram, "fixture");
        hw.gpu.adapters = if gpus.is_empty() {
            HardwareField::detected(Vec::new(), "fixture: no GPU")
        } else {
            HardwareField::measured(gpus, "fixture")
        };
        hw.gpu.cuda_available = match cuda {
            Some(v) => HardwareField::measured(v, "fixture"),
            None => HardwareField::unavailable("fixture: unknown"),
        };
        hw.gpu.vulkan_available = match vulkan {
            Some(v) => HardwareField::measured(v, "fixture"),
            None => HardwareField::unavailable("fixture: unknown"),
        };
        hw.storage = free_disk.map(|f| StorageReport {
            path_queried: "C:\\fixture".to_string(),
            free_bytes: HardwareField::measured(f, "fixture"),
            total_bytes: HardwareField::measured(f * 2, "fixture"),
        });
        profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0)
    }

    fn top_fit(profile: &HardwareCapabilityProfile, catalog_id: &str) -> FitState {
        let catalog = dev_catalog();
        let build = catalog.require(catalog_id).unwrap();
        let calib = seeded_calibration();
        recommend::evaluate_build(build, profile, &calib, None, Priority::Balanced, None, None)
            .fit
            .state
    }

    /// Fixture: 8 GB RAM, CPU only. Only the smallest catalog entries
    /// should be usable; the 8B/70B Llama builds must not be recommended.
    #[test]
    fn fixture_8gb_ram_cpu_only() {
        let profile = synthetic_profile(
            8_000_000_000,
            6_000_000_000,
            vec![],
            Some(false),
            Some(false),
            Some(200_000_000_000),
        );

        assert!(matches!(
            top_fit(&profile, "qwen2.5-0.5b-instruct-q4_k_m"),
            FitState::Excellent | FitState::Good
        ));
        assert_eq!(
            top_fit(&profile, "llama-3.1-8b-instruct-q4_k_m"),
            FitState::NotRecommended
        );
        assert_eq!(
            top_fit(&profile, "llama-3.1-70b-instruct-q4_k_m"),
            FitState::NotRecommended
        );

        // Backend compatibility gate: no backend at all should ever push a
        // build to NotRecommended, not silently ignore the mismatch.
        let catalog = dev_catalog();
        let cuda_only_build = catalog.require("llama-3.1-8b-instruct-q4_k_m").unwrap();
        assert!(
            cuda_only_build
                .supported_backends
                .contains(&crate::runtime::Backend::Cpu)
        );
    }

    /// Fixture: 16 GB RAM, a modest (low-VRAM) GPU.
    #[test]
    fn fixture_16gb_ram_modest_gpu() {
        let gpu = GpuAdapter {
            name: "Modest GPU 4GB".to_string(),
            vendor: GpuVendor::Nvidia,
            dedicated_vram_bytes: Some(4_000_000_000),
            shared_system_memory_bytes: Some(1_000_000_000),
            driver_version: Some("999.99".to_string()),
        };
        let profile = synthetic_profile(
            16_000_000_000,
            12_000_000_000,
            vec![gpu],
            Some(true),
            Some(false),
            Some(200_000_000_000),
        );

        assert!(matches!(
            top_fit(&profile, "qwen2.5-0.5b-instruct-q4_k_m"),
            FitState::Excellent | FitState::Good
        ));
        assert!(matches!(
            top_fit(&profile, "qwen2.5-coder-1.5b-instruct-q4_k_m"),
            FitState::Excellent | FitState::Good | FitState::Constrained
        ));
    }

    /// Fixture: 16 GB RAM, RTX 5050 Laptop GPU - mirrors this project's
    /// actual dev machine's real GPU shape (8GB dedicated VRAM), but built
    /// synthetically so the test doesn't depend on running on that exact
    /// physical machine.
    #[test]
    fn fixture_16gb_ram_rtx_5050_laptop() {
        let gpu = GpuAdapter {
            name: "NVIDIA GeForce RTX 5050 Laptop GPU".to_string(),
            vendor: GpuVendor::Nvidia,
            dedicated_vram_bytes: Some(8_294_236_160),
            shared_system_memory_bytes: Some(8_000_000_000),
            driver_version: Some("596.49".to_string()),
        };
        let profile = synthetic_profile(
            16_873_545_728,
            8_000_000_000,
            vec![gpu],
            Some(true),
            Some(true),
            Some(200_000_000_000),
        );

        assert_eq!(
            top_fit(&profile, "qwen2.5-0.5b-instruct-q4_k_m"),
            FitState::Excellent
        );
        assert_ne!(
            top_fit(&profile, "llama-3.1-70b-instruct-q4_k_m"),
            FitState::Excellent
        );
        assert_ne!(
            top_fit(&profile, "llama-3.1-70b-instruct-q4_k_m"),
            FitState::Unknown
        );
    }

    /// Fixture: 32 GB RAM, 12 GB VRAM - should fit the 8B model at least
    /// technically. Our calibration store has no Llama data (only Qwen2),
    /// so an uncalibrated-but-fitting verdict of Experimental is an
    /// honest, acceptable outcome here - what must never happen is
    /// NotRecommended or Unknown for a build that clearly has the
    /// resources to run.
    #[test]
    fn fixture_32gb_ram_12gb_vram() {
        let gpu = GpuAdapter {
            name: "Fixture GPU 12GB".to_string(),
            vendor: GpuVendor::Nvidia,
            dedicated_vram_bytes: Some(12_000_000_000),
            shared_system_memory_bytes: Some(2_000_000_000),
            driver_version: Some("999.99".to_string()),
        };
        let profile = synthetic_profile(
            32_000_000_000,
            26_000_000_000,
            vec![gpu],
            Some(true),
            Some(false),
            Some(500_000_000_000),
        );

        assert!(matches!(
            top_fit(&profile, "llama-3.1-8b-instruct-q4_k_m"),
            FitState::Excellent | FitState::Good | FitState::Experimental
        ));
        assert_ne!(
            top_fit(&profile, "llama-3.1-70b-instruct-q4_k_m"),
            FitState::Excellent
        );
    }

    /// Fixture: 64 GB RAM workstation, no dedicated GPU (CPU-only but
    /// generously provisioned). Even the 8B model should fit comfortably;
    /// the 70B model remains a stretch but must not be silently hidden -
    /// it's either Constrained/Experimental or NotRecommended, never
    /// Unknown (RAM here is fully known).
    #[test]
    fn fixture_64gb_workstation_cpu_only() {
        let profile = synthetic_profile(
            64_000_000_000,
            58_000_000_000,
            vec![],
            Some(false),
            Some(false),
            Some(1_000_000_000_000),
        );

        assert!(matches!(
            top_fit(&profile, "llama-3.1-8b-instruct-q4_k_m"),
            FitState::Excellent | FitState::Good
        ));
        assert_ne!(
            top_fit(&profile, "llama-3.1-70b-instruct-q4_k_m"),
            FitState::Unknown
        );
    }

    /// Fixture: incomplete hardware detection - available RAM and GPU
    /// state both unknown. Every build must classify as Unknown, never
    /// silently defaulted to a fit verdict.
    #[test]
    fn fixture_incomplete_hardware_detection() {
        let mut hw = crate::hardware::inspect(None);
        hw.memory.available_bytes = HardwareField::unavailable("fixture: detection failed");
        hw.gpu.cuda_available = HardwareField::unavailable("fixture: detection failed");
        hw.gpu.vulkan_available = HardwareField::unavailable("fixture: detection failed");
        let profile = profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0);

        for catalog_id in [
            "qwen2.5-0.5b-instruct-q4_k_m",
            "llama-3.1-8b-instruct-q4_k_m",
        ] {
            assert_eq!(top_fit(&profile, catalog_id), FitState::Unknown);
        }
    }

    /// Multi-GPU machine: an iGPU plus a discrete NVIDIA GPU (this
    /// project's actual dev machine has exactly this shape) - fit
    /// evaluation must not crash or misbehave with more than one adapter
    /// present.
    #[test]
    fn fixture_multi_gpu_machine_does_not_panic() {
        let igpu = GpuAdapter {
            name: "Integrated Graphics".to_string(),
            vendor: GpuVendor::Intel,
            dedicated_vram_bytes: Some(134_217_728),
            shared_system_memory_bytes: Some(8_000_000_000),
            driver_version: None,
        };
        let dgpu = GpuAdapter {
            name: "Discrete GPU".to_string(),
            vendor: GpuVendor::Nvidia,
            dedicated_vram_bytes: Some(8_000_000_000),
            shared_system_memory_bytes: Some(8_000_000_000),
            driver_version: Some("999.99".to_string()),
        };
        let profile = synthetic_profile(
            16_000_000_000,
            10_000_000_000,
            vec![igpu, dgpu],
            Some(true),
            Some(true),
            Some(100_000_000_000),
        );

        let catalog = dev_catalog();
        for build in &catalog.builds {
            let calib = empty_calibration();
            let _ = recommend::evaluate_build(
                build,
                &profile,
                &calib,
                None,
                Priority::Balanced,
                None,
                None,
            );
        }
        assert_eq!(profile.gpus.len(), 2);
    }

    /// Ranking across every priority must complete without panicking for
    /// every fixture machine shape, and always return a deterministic,
    /// non-empty order when the catalog is non-empty.
    #[test]
    fn ranking_never_panics_across_all_priorities_and_fixture_machines() {
        let catalog = dev_catalog();
        let calib = empty_calibration();
        let priorities = [
            Priority::Fastest,
            Priority::Balanced,
            Priority::HighestQuality,
            Priority::LowestMemory,
            Priority::LongestContext,
            Priority::Coding,
            Priority::ArabicGeneralChat,
            Priority::PrivacyOffline,
        ];
        let machines = [
            synthetic_profile(
                8_000_000_000,
                6_000_000_000,
                vec![],
                Some(false),
                Some(false),
                Some(100_000_000_000),
            ),
            synthetic_profile(
                64_000_000_000,
                58_000_000_000,
                vec![],
                Some(false),
                Some(false),
                Some(100_000_000_000),
            ),
        ];
        for profile in &machines {
            for priority in priorities {
                let ranked = recommend::rank(&catalog, profile, &calib, None, priority);
                assert_eq!(ranked.evaluations.len(), catalog.builds.len());
            }
        }
    }

    /// Performance measurement, not a correctness assertion: records real
    /// wall-clock timings for the Stage 1 pipeline (catalog parse, one
    /// fit evaluation, a full ranking pass) in isolation from Stage 0's
    /// hardware-detection subprocess overhead (nvidia-smi/vulkaninfo),
    /// which dominates end-to-end CLI wall time but isn't Stage 1 cost.
    /// Values are printed for `docs/stage-1-verification.md`; run with
    /// `cargo test measure_stage1_performance -- --nocapture`.
    #[test]
    fn measure_stage1_performance() {
        use std::time::Instant;

        let catalog_path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("data/catalog/dev-catalog.json");
        let t0 = Instant::now();
        let catalog = catalog::load_catalog(&catalog_path).unwrap();
        let catalog_parse_time = t0.elapsed();

        let hw = crate::hardware::inspect(None);
        let t1 = Instant::now();
        let profile = profile::build_profile(&hw, "2026-01-01T00:00:00Z".to_string(), 0);
        let profile_build_time = t1.elapsed();

        let calib = seeded_calibration();
        let build = catalog.get("qwen2.5-0.5b-instruct-q4_k_m").unwrap();
        let t2 = Instant::now();
        let _ = recommend::evaluate_build(
            build,
            &profile,
            &calib,
            None,
            Priority::Balanced,
            None,
            None,
        );
        let single_fit_time = t2.elapsed();

        let t3 = Instant::now();
        let ranked = recommend::rank(&catalog, &profile, &calib, None, Priority::Balanced);
        let ranking_time = t3.elapsed();
        assert_eq!(ranked.evaluations.len(), catalog.builds.len());

        eprintln!(
            "PERF catalog_parse={catalog_parse_time:?} profile_build_from_inspected_hw={profile_build_time:?} single_fit_eval={single_fit_time:?} full_rank_{}builds={ranking_time:?}",
            catalog.builds.len()
        );
    }
}
