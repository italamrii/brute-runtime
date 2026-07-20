import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Profiles } from "./Profiles";
import type { RuntimeProfile } from "../lib/types";

vi.mock("../lib/api", () => ({
  listProfiles: vi.fn(),
  showProfile: vi.fn(),
  listLibrary: vi.fn(),
  deleteProfile: vi.fn(),
  exportProfile: vi.fn(),
  verifyProfile: vi.fn(),
}));

import { listLibrary, listProfiles, showProfile } from "../lib/api";

function runtimeProfile(): RuntimeProfile {
  return {
    schema_version: "v1",
    profile_id: "profile-instance-f052be34d631ff2889844a7551eea020",
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
    source_repetitions_succeeded: 5,
    source_repetitions_requested: 5,
  };
}

function renderProfiles(lang: "en" | "ar" = "en") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <AppStatusProvider>
        <Profiles />
      </AppStatusProvider>
    </I18nProvider>,
  );
}

describe("Profiles page", () => {
  beforeEach(() => {
    vi.mocked(listProfiles).mockResolvedValue(["profile-instance-f052be34d631ff2889844a7551eea020"]);
    vi.mocked(showProfile).mockResolvedValue(runtimeProfile());
    vi.mocked(listLibrary).mockResolvedValue([]);
  });

  it("keeps the profile ID, backend, and stability value forced ltr in the Arabic RTL page", async () => {
    renderProfiles("ar");
    fireEvent.click(await screen.findByText("profile-instance-f052be34d631ff2889844a7551eea020"));
    await waitFor(() => expect(showProfile).toHaveBeenCalled());

    // The profile ID appears both in the list button and the inspector
    // title - both instances get the fix.
    for (const idEl of screen.getAllByText("profile-instance-f052be34d631ff2889844a7551eea020")) {
      expect(idEl).toHaveAttribute("dir", "ltr");
    }
    expect(screen.getByText("cpu")).toHaveAttribute("dir", "ltr");
    expect(screen.getByText("stable")).toHaveAttribute("dir", "ltr");
    expect(screen.getByText("high")).toHaveAttribute("dir", "ltr");
  });
});
