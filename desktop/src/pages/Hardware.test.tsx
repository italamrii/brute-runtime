import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { Hardware } from "./Hardware";
import type { HardwareCapabilityProfile, CalibrationStore } from "../lib/types";

vi.mock("../lib/api", () => ({
  getHardwareProfile: vi.fn(),
  listCalibrations: vi.fn(),
}));

import { getHardwareProfile, listCalibrations } from "../lib/api";

function field<T>(value: T): { value: T; confidence: "measured"; source: string } {
  return { value, confidence: "measured", source: "test" };
}

function hardwareProfile(): HardwareCapabilityProfile {
  return {
    schema_version: "v1",
    machine_id: "machine-1",
    captured_at_rfc3339: "2026-01-01T00:00:00Z",
    os: {
      product_name: field("Windows 11 Home"),
      display_version: field("25H2"),
      build_number: field("26200"),
      process_architecture: field("x64"),
      native_architecture: field("x64"),
    },
    cpu: {
      vendor: field("GenuineIntel"),
      brand: field("13th Gen Intel(R) Core(TM) i5-13450HX"),
      physical_cores: field(10),
      logical_cores: field(16),
      instruction_sets: field(["AVX2", "AVX512"]),
    },
    memory: {
      total_bytes: field(17_179_869_184),
      available_bytes: field(8_589_934_592),
    },
    gpus: [
      { name: "NVIDIA GeForce RTX 5050 Laptop GPU", vendor: "nvidia", dedicated_vram_bytes: 8_589_934_592, shared_system_memory_bytes: null, driver_version: "560.94" },
    ],
    backends: { cpu: true, cuda: field(true), vulkan: field(true) },
    storage: {
      path_queried: "\\\\?\\C:\\",
      free_bytes: field(100_000_000_000),
      total_bytes: field(500_000_000_000),
    },
    power: {
      ac_line_status: field("online"),
      battery_percent: field(100),
      battery_present: field(true),
      chassis_class: field("laptop"),
    },
    calibration_record_count: 1,
    confidence_summary: { measured_count: 5, detected_count: 2, inferred_count: 0, unavailable_count: 0 },
  };
}

function calibrationStore(): CalibrationStore {
  return { records: [] };
}

function renderHardware(lang: "en" | "ar" = "en") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <Hardware />
    </I18nProvider>,
  );
}

describe("Hardware page", () => {
  beforeEach(() => {
    vi.mocked(getHardwareProfile).mockResolvedValue(hardwareProfile());
    vi.mocked(listCalibrations).mockResolvedValue(calibrationStore());
  });

  it("keeps the CPU brand, driver version, and storage path forced ltr in the Arabic RTL page", async () => {
    renderHardware("ar");
    const brand = await screen.findAllByText("13th Gen Intel(R) Core(TM) i5-13450HX");
    for (const el of brand) {
      expect(el).toHaveAttribute("dir", "ltr");
    }

    const driver = screen.getByText("560.94");
    expect(driver).toHaveAttribute("dir", "ltr");

    const storagePath = screen.getByText("\\\\?\\C:\\");
    expect(storagePath).toHaveAttribute("dir", "ltr");

    const isa = screen.getByText("AVX2, AVX512");
    expect(isa).toHaveAttribute("dir", "ltr");
  });
});
