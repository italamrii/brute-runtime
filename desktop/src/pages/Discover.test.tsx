import { StrictMode } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { Discover } from "./Discover";
import type { ModelBuild, BuildEvaluation } from "../lib/types";

const openUrlMock = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));

vi.mock("../lib/api", () => ({
  listCatalog: vi.fn(),
  evaluateFit: vi.fn(),
}));

import { listCatalog, evaluateFit } from "../lib/api";

function build(overrides: Partial<ModelBuild> = {}): ModelBuild {
  return {
    catalog_id: "test-model-q4",
    family: "test-model",
    display_name: "Test Model 7B",
    publisher: "Test Publisher",
    official_source_url: "https://huggingface.co/example/test-model-gguf",
    official_repository_id: "example/test-model-gguf",
    filename: "test-model.Q4_K_M.gguf",
    architecture: "qwen2",
    parameter_count: 7_000_000_000,
    quantization: "Q4_K_M",
    file_size_bytes: 4_500_000_000,
    estimated_disk_bytes: null,
    estimated_runtime_memory_bytes: null,
    min_recommended_ram_bytes: 8_000_000_000,
    min_recommended_vram_bytes: null,
    supported_backends: ["cpu"],
    context_sizes: [4096],
    task_categories: ["general_chat"],
    short_description: "A test model.",
    strength: "General purpose.",
    limitation: "None noted.",
    license: { status: "known", identifier: "apache-2.0" },
    commercial_use: "unknown",
    gated_access: null,
    metadata_provenance: "test fixture",
    last_reviewed: "2026-01-01",
    ...overrides,
  };
}

function evaluation(b: ModelBuild): BuildEvaluation {
  return {
    build: b,
    estimate: {
      formula_version: "v1",
      quality: "coarse_approximation",
      model_weights_bytes: { value: 4_500_000_000, provenance: "measured", note: "test" },
      kv_cache_bytes_low: { value: 100, provenance: "measured", note: "test" },
      kv_cache_bytes_high: { value: 200, provenance: "measured", note: "test" },
      runtime_overhead_bytes_low: 0,
      runtime_overhead_bytes_high: 0,
      os_safety_reserve_bytes: 0,
      estimated_total_ram_bytes_low: 6_000_000_000,
      estimated_total_ram_bytes_high: 7_000_000_000,
      estimated_vram_bytes_low: null,
      estimated_vram_bytes_high: null,
    },
    fit: { state: "good", reasons: [], headroom_ratio: null, rules_version: "v1" },
    calibration_match: null,
    component_scores: {},
    score: 0.8,
  };
}

function renderDiscover() {
  return render(
    <I18nProvider>
      <Discover />
    </I18nProvider>,
  );
}

describe("Discover page — model card navigation safety", () => {
  beforeEach(() => {
    openUrlMock.mockReset();
    const b = build();
    vi.mocked(listCatalog).mockResolvedValue([b]);
    vi.mocked(evaluateFit).mockResolvedValue(evaluation(b));
  });

  it("clicking a model card opens an internal modal, not a navigation", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(within(dialog).getByText("Test Model 7B")).toBeInTheDocument();
  });

  it("shows internal details (params, quant, license) inside the dialog", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Q4_K_M")).toBeInTheDocument();
  });

  // Regression test for a real crash found in the installed build: the
  // real Rust `License` enum serializes as {"status":"known","identifier"}
  // or {"status":"unknown"} (verified directly against
  // data/catalog/dev-catalog.json) - an earlier, wrong TS type/ternary
  // assumed a bare "unknown" string or {known:{identifier}}, which threw
  // on every real catalog entry and crashed the whole React tree with no
  // recovery. This fixture intentionally uses the real wire shape for
  // both license variants so a future shape mismatch fails here instead
  // of only in a live installed build.
  it("renders a known-license model's details without crashing", async () => {
    const b = build({ license: { status: "known", identifier: "apache-2.0" } });
    vi.mocked(listCatalog).mockResolvedValue([b]);
    vi.mocked(evaluateFit).mockResolvedValue(evaluation(b));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("apache-2.0")).toBeInTheDocument();
  });

  it("renders an unknown-license model's details without crashing", async () => {
    const b = build({ license: { status: "unknown" } });
    vi.mocked(listCatalog).mockResolvedValue([b]);
    vi.mocked(evaluateFit).mockResolvedValue(evaluation(b));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("License")).toBeInTheDocument();
    expect(within(dialog).getAllByText("unknown").length).toBeGreaterThan(0);
  });

  it("Close button closes the modal and returns to the model grid", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    await screen.findByRole("dialog");
    fireEvent.click(screen.getAllByText("Close")[0]);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("Escape key closes the modal", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    await screen.findByRole("dialog");
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("Back returns from the open-in-browser confirmation to details", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    await screen.findByText("Open in browser");
    fireEvent.click(screen.getByText("Back"));
    expect(await screen.findByText("Q4_K_M")).toBeInTheDocument();
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("Cancel on the confirmation closes the modal without opening the URL", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    await screen.findByText("Open in browser");
    fireEvent.click(screen.getByText("Cancel"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("confirming opens the real https source via the system browser, never a WebView navigation", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    await screen.findByText("Open in browser");
    fireEvent.click(screen.getByText("Open in browser"));
    // openUrl hands the link to the OS's real default browser as a
    // separate process - it is the only mechanism this flow ever calls to
    // reach an external URL; the BRUTE window itself never navigates.
    await waitFor(() => expect(openUrlMock).toHaveBeenCalledWith("https://huggingface.co/example/test-model-gguf"));
    expect(openUrlMock).toHaveBeenCalledTimes(1);
  });

  it("rejects a localhost official_source_url instead of opening it", async () => {
    const badBuild = build({ official_source_url: "http://localhost:1420/" });
    vi.mocked(listCatalog).mockResolvedValue([badBuild]);
    vi.mocked(evaluateFit).mockResolvedValue(evaluation(badBuild));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    expect(await screen.findByText("This link was not opened")).toBeInTheDocument();
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("rejects a malformed official_source_url instead of opening it", async () => {
    const badBuild = build({ official_source_url: "not-a-url" });
    vi.mocked(listCatalog).mockResolvedValue([badBuild]);
    vi.mocked(evaluateFit).mockResolvedValue(evaluation(badBuild));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    expect(await screen.findByText("This link was not opened")).toBeInTheDocument();
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("a rejected-URL dialog still offers Back/Close so the user is never trapped", async () => {
    const badBuild = build({ official_source_url: "javascript:alert(1)" });
    vi.mocked(listCatalog).mockResolvedValue([badBuild]);
    vi.mocked(evaluateFit).mockResolvedValue(evaluation(badBuild));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    await screen.findByText("This link was not opened");
    fireEvent.click(screen.getByText("Back"));
    expect(await screen.findByText("Q4_K_M")).toBeInTheDocument();
  });

  it("the 'verified source only' filter actually excludes unknown-license models", async () => {
    const known = build({ catalog_id: "known-1", display_name: "Known License Model" });
    const unknown = build({
      catalog_id: "unknown-1",
      display_name: "Unknown License Model",
      license: { status: "unknown" },
    });
    vi.mocked(listCatalog).mockResolvedValue([known, unknown]);
    vi.mocked(evaluateFit).mockImplementation(async (id: string) =>
      evaluation(id === "known-1" ? known : unknown),
    );
    renderDiscover();
    await screen.findByText("Known License Model");
    expect(screen.getByText("Unknown License Model")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Verified source only"));
    expect(screen.getByText("Known License Model")).toBeInTheDocument();
    expect(screen.queryByText("Unknown License Model")).not.toBeInTheDocument();
  });

  it("a single click creates exactly one dialog, never more", async () => {
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    await screen.findByRole("dialog");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  it("rapid double click on the same card still creates exactly one dialog", async () => {
    renderDiscover();
    const card = await screen.findByText("Test Model 7B");
    fireEvent.click(card);
    fireEvent.click(card);
    await screen.findByRole("dialog");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  it("repeated open/close cycles never accumulate extra dialogs", async () => {
    renderDiscover();
    const card = await screen.findByText("Test Model 7B");
    for (let i = 0; i < 5; i++) {
      fireEvent.click(card);
      await screen.findByRole("dialog");
      expect(screen.getAllByRole("dialog")).toHaveLength(1);
      fireEvent.click(screen.getAllByText("Close")[0]);
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    }
  });

  it("repeated open/close cycles never leak Escape-key listeners (closes cleanly every time)", async () => {
    renderDiscover();
    const card = await screen.findByText("Test Model 7B");
    for (let i = 0; i < 3; i++) {
      fireEvent.click(card);
      await screen.findByRole("dialog");
      fireEvent.keyDown(window, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    }
    // If a prior cycle's listener leaked, closing would now double-fire
    // side effects (harmless here) but never a second/duplicate dialog.
    expect(screen.queryAllByRole("dialog")).toHaveLength(0);
  });

  it("survives React.StrictMode's double-invoked effects without duplicating the dialog", async () => {
    render(
      <StrictMode>
        <I18nProvider>
          <Discover />
        </I18nProvider>
      </StrictMode>,
    );
    fireEvent.click(await screen.findByText("Test Model 7B"));
    await screen.findByRole("dialog");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });
});
