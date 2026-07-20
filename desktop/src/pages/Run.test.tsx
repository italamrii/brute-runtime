import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Run } from "./Run";
import type { LibraryEntry, RuntimeProfile } from "../lib/types";

vi.mock("../lib/api", () => ({
  listLibrary: vi.fn(),
  getAssociations: vi.fn(),
  localRunGenerate: vi.fn(),
  localRunCancel: vi.fn(),
}));

import { getAssociations, listLibrary } from "../lib/api";

function libraryEntry(): LibraryEntry {
  return {
    library_id: "lib-1",
    schema_version: "v1",
    sha256: "a".repeat(64),
    file_size_bytes: 500_000_000,
    gguf_version: 3,
    tensor_count: 10,
    kv_count: 5,
    architecture: "qwen2",
    quantization: "Q4_K_M",
    parameter_count: 500_000_000,
    current_path: "C:\\Models\\qwen.gguf",
    original_import_path: "C:\\Models\\qwen.gguf",
    imported_at_rfc3339: "2026-01-01T00:00:00Z",
    last_verified_at_rfc3339: "2026-01-01T00:00:00Z",
    file_modified_at_rfc3339: null,
    file_status: "unchanged",
    trust: "local_unverified_source",
    last_verification: null,
    catalog_match: { catalog_id: null, confidence: "none", notes: [] },
    alias: "Qwen2.5 0.5B",
    notes: null,
    quarantine: null,
    managed_copy: false,
  };
}

function runtimeProfile(): RuntimeProfile {
  return {
    schema_version: "v1",
    profile_id: "profile-1",
    tuning_date: "2026-01-01T00:00:00Z",
    model_sha256: "a".repeat(64),
    model_architecture: "qwen2",
    model_quantization: "Q4_K_M",
    model_parameter_count: 500_000_000,
    machine_profile_schema_version: "v1",
    machine_id: "machine-1",
    backend: "cpu",
    llama_cli_sha256: "b".repeat(64),
    llama_bench_sha256: "b".repeat(64),
    threads: 8,
    gpu_layers: 0,
    context_size: 4096,
    batch_size: 512,
    mean_generation_tokens_per_second: 40,
    mean_prompt_tokens_per_second: 300,
    predicted_ram_bytes: 1_000_000_000,
    predicted_vram_bytes: null,
    stability: "stable",
    stability_formula_version: "v1",
    ranking_formula_version: "v1",
    tuning_formula_version: "v1",
    confidence: "high",
  } as RuntimeProfile;
}

function renderRun(lang: "en" | "ar" = "en") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <AppStatusProvider>
        <Run />
      </AppStatusProvider>
    </I18nProvider>,
  );
}

describe("Run page", () => {
  beforeEach(() => {
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry()]);
    vi.mocked(getAssociations).mockResolvedValue({ runtime_profiles: [runtimeProfile()], calibration_record_count: 1 });
  });

  it("keeps the backend, threads, and runtime binary path forced ltr in the Arabic RTL page", async () => {
    renderRun("ar");
    fireEvent.change(await screen.findByLabelText("النموذج"), { target: { value: "lib-1" } });
    await waitFor(() => expect(getAssociations).toHaveBeenCalled());
    await screen.findByText("cpu");

    expect(screen.getByText("cpu")).toHaveAttribute("dir", "ltr");
    expect(screen.getByText("8")).toHaveAttribute("dir", "ltr");
    expect(screen.getByText("40.0 tok/s")).toHaveAttribute("dir", "ltr");
  });

  it("keeps the model and runtime-profile picker options ltr-oriented", async () => {
    renderRun("ar");
    const modelSelect = await screen.findByLabelText("النموذج");
    const modelOption = modelSelect.querySelector("option[value='lib-1']");
    expect(modelOption).toHaveAttribute("dir", "ltr");

    fireEvent.change(modelSelect, { target: { value: "lib-1" } });
    await waitFor(() => expect(getAssociations).toHaveBeenCalled());
    const profileSelect = await screen.findByLabelText("إعداد التشغيل");
    const profileOption = profileSelect.querySelector("option[value='profile-1']");
    expect(profileOption).toHaveAttribute("dir", "ltr");
  });
});
