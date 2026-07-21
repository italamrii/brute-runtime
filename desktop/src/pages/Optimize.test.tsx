import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Optimize } from "./Optimize";
import type { ModelBuild, Recommendation, RecommendationDto } from "../lib/types";

vi.mock("../lib/api", () => ({
  recommendModel: vi.fn(),
  listLibrary: vi.fn().mockResolvedValue([]),
  tuneDryRun: vi.fn(),
  tuneRun: vi.fn(),
  tuneCancel: vi.fn(),
}));

import { recommendModel } from "../lib/api";

function build(): ModelBuild {
  return {
    catalog_id: "qwen2.5-0.5b-instruct-q4_k_m",
    family: "qwen2.5-0.5b-instruct",
    display_name: "Qwen2.5 0.5B Instruct (Q4_K_M)",
    publisher: "Alibaba Cloud (Qwen Team)",
    official_source_url: "https://huggingface.co/Qwen/Qwen2.5-0.5B-Instruct-GGUF",
    official_repository_id: "Qwen/Qwen2.5-0.5B-Instruct-GGUF",
    filename: "qwen2.5-0.5b-instruct-q4_k_m.gguf",
    architecture: "qwen2",
    parameter_count: 630_000_000,
    quantization: "Q4_K_M",
    file_size_bytes: 469_000_000,
    estimated_disk_bytes: null,
    estimated_runtime_memory_bytes: null,
    min_recommended_ram_bytes: 2_000_000_000,
    min_recommended_vram_bytes: null,
    supported_backends: ["cpu"],
    context_sizes: [4096],
    task_categories: ["general_chat"],
    short_description: "A small general chat model.",
    strength: "Fast on CPU.",
    limitation: "Limited reasoning depth.",
    license: { status: "known", identifier: "apache-2.0" },
    commercial_use: "allowed",
    gated_access: null,
    metadata_provenance: "verified",
    last_reviewed: "2026-01-01",
    family_id: null,
    model_id: null,
    artifact_id: null,
    source_repository: null,
    exact_model_name: null,
    version: null,
    context_length: null,
    file_format: null,
    runtime_provider: null,
    minimum_runtime_version: null,
    license_url: null,
    source_verification: "unknown",
    artifact_verification: "unknown",
    exact_artifact_url: null,
    checksum_algorithm: null,
    checksum_value: null,
    checksum_source: null,
    curator_notes: null,
    arabic_capability: "unknown",
    coding_capability: "unknown",
    reasoning_capability: "unknown",
    general_quality: "unknown",
    speed_category: "unknown",
    evidence_source: "unknown",
    benchmark_confidence: null,
  };
}

function recommendation(): Recommendation {
  const b = build();
  const evaluation = {
    build: b,
    estimate: {
      formula_version: "v1",
      quality: "coarse_approximation" as const,
      model_weights_bytes: { value: 469_000_000, provenance: "measured" as const, note: "test" },
      kv_cache_bytes_low: { value: 100, provenance: "measured" as const, note: "test" },
      kv_cache_bytes_high: { value: 200, provenance: "measured" as const, note: "test" },
      runtime_overhead_bytes_low: 0,
      runtime_overhead_bytes_high: 0,
      os_safety_reserve_bytes: 0,
      estimated_total_ram_bytes_low: 900_000_000,
      estimated_total_ram_bytes_high: 1_100_000_000,
      estimated_vram_bytes_low: null,
      estimated_vram_bytes_high: null,
    },
    fit: { state: "excellent" as const, reasons: [], headroom_ratio: null, rules_version: "v1" },
    calibration_match: null,
    component_scores: {},
    score: 0.9,
  };
  return {
    recommended: evaluation,
    safer_fallback: null,
    stronger_optional: null,
    explanation: {
      simple: "This model fits comfortably on your device.",
      technical: {
        detected_resources: "16 GB RAM detected",
        memory_calculation: "469 MB weights + overhead",
        calibration_used: null,
        backend_assumptions: "cpu",
        context_and_batch_assumptions: "context=4096 batch=512",
        fit_thresholds: "excellent >= 50% headroom",
        confidence_calculation: "high",
      },
    },
  };
}

function renderOptimize(lang: "en" | "ar" = "en") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <AppStatusProvider>
        <Optimize />
      </AppStatusProvider>
    </I18nProvider>,
  );
}

describe("Optimize page — Recommend tab", () => {
  beforeEach(() => {
    const dto: RecommendationDto = { recommendation: recommendation(), ranking_formula_version: "stage1-v1" };
    vi.mocked(recommendModel).mockResolvedValue(dto);
  });

  it("keeps the model name, fit state, and technical explanation forced ltr in the Arabic RTL page", async () => {
    renderOptimize("ar");
    // "توصية" is reused for the tab label, the panel title, and the
    // submit button - target the actual primary submit button.
    const candidates = await screen.findAllByText("توصية");
    const submitButton = candidates.find((el) => el.tagName === "BUTTON" && el.classList.contains("btn-primary"));
    if (!submitButton) throw new Error("submit button not found");
    fireEvent.click(submitButton);
    await waitFor(() => expect(recommendModel).toHaveBeenCalled());

    const name = await screen.findByText("Qwen2.5 0.5B Instruct (Q4_K_M)");
    expect(name).toHaveAttribute("dir", "ltr");

    const fitState = screen.getByText("excellent");
    expect(fitState).toHaveAttribute("dir", "ltr");

    // Switch to the technical explanation tab to reach the raw kv dump.
    fireEvent.click(screen.getByText(/تقني|Technical/));
    const backendAssumption = screen.getByText("cpu");
    expect(backendAssumption).toHaveAttribute("dir", "ltr");
  });

  it("keeps the priority <option> values ltr-oriented", async () => {
    renderOptimize("ar");
    const select = await screen.findByLabelText(/الأولوية|Priority/);
    const option = select.querySelector("option[value='balanced']");
    expect(option).toHaveAttribute("dir", "ltr");
  });
});
