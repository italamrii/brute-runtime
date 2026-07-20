import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { useEffect } from "react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider, useAppStatus } from "../lib/AppStatusContext";
import { AdvancedConsole } from "./AdvancedConsole";
import type { LibraryEntry, RuntimeProfile } from "../lib/types";

vi.mock("../lib/api", () => ({
  resolveRuntime: vi.fn(),
  listLibrary: vi.fn(),
  getAssociations: vi.fn(),
}));

import { getAssociations, listLibrary, resolveRuntime } from "../lib/api";

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

function ActivateStatus({ modelId, profileId }: { modelId: string | null; profileId: string | null }) {
  const status = useAppStatus();
  useEffect(() => {
    status.setActiveModel(modelId, modelId ? "Qwen2.5 0.5B" : null);
    status.setActiveProfile(profileId, profileId ? "cpu" : null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
  return null;
}

function renderConsole(modelId: string | null, profileId: string | null, onNavigate = vi.fn()) {
  return {
    onNavigate,
    ...render(
      <I18nProvider>
        <AppStatusProvider>
          <ActivateStatus modelId={modelId} profileId={profileId} />
          <AdvancedConsole onNavigate={onNavigate} />
        </AppStatusProvider>
      </I18nProvider>,
    ),
  };
}

describe("Advanced Console", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(resolveRuntime).mockResolvedValue({
      source: "bundled",
      binary_dir: "C:\\Program Files\\BRUTE Runtime\\runtime\\cpu",
      cli_verified: true,
      bench_verified: true,
      detail: "Bundled runtime verified.",
    });
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry()]);
    vi.mocked(getAssociations).mockResolvedValue({ runtime_profiles: [runtimeProfile()], calibration_record_count: 1 });
  });

  it("shows an honest empty state when nothing is active yet, not fabricated data", async () => {
    renderConsole(null, null);
    await screen.findByText(/No active model yet/);
    expect(screen.getByText(/No active tuning profile yet/)).toBeInTheDocument();
  });

  it("shows real runtime resolution facts", async () => {
    renderConsole(null, null);
    await waitFor(() => expect(resolveRuntime).toHaveBeenCalled());
    expect(await screen.findByText("bundled")).toBeInTheDocument();
    expect(screen.getByText("C:\\Program Files\\BRUTE Runtime\\runtime\\cpu")).toBeInTheDocument();
  });

  it("shows the real active model's facts once one is set", async () => {
    renderConsole("lib-1", null);
    expect(await screen.findByText("Qwen2.5 0.5B")).toBeInTheDocument();
    expect(screen.getByText("qwen2")).toBeInTheDocument();
    expect(screen.getByText("Q4_K_M")).toBeInTheDocument();
    expect(screen.getByText("500M")).toBeInTheDocument();
    expect(screen.getByText("local_unverified_source")).toBeInTheDocument();
  });

  it("shows the real active profile's facts once one is set", async () => {
    renderConsole("lib-1", "profile-1");
    expect(await screen.findByText("profile-1")).toBeInTheDocument();
    expect(screen.getByText("8")).toBeInTheDocument();
    expect(screen.getByText("4096")).toBeInTheDocument();
    expect(screen.getByText("40.0 tok/s")).toBeInTheDocument();
    expect(screen.getByText("high")).toBeInTheDocument();
  });

  it("navigates to the Run page when asked to open the interactive console", async () => {
    const onNavigate = vi.fn();
    renderConsole(null, null, onNavigate);
    const btn = await screen.findByText("Open the interactive Run console");
    btn.click();
    expect(onNavigate).toHaveBeenCalledWith("run");
  });
});
