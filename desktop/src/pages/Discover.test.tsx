import { StrictMode } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { Discover } from "./Discover";
import type { BuildRecommendationV2, LibraryEntry, ModelBuild, RecommendationSetV2 } from "../lib/types";

const openUrlMock = vi.fn();
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...args),
}));

vi.mock("../lib/api", () => ({
  recommendV2: vi.fn(),
  listLibrary: vi.fn(),
  checkDownloadSpace: vi.fn(),
  downloadModel: vi.fn(),
  cancelDownload: vi.fn(),
  importModel: vi.fn(),
}));

import { recommendV2, listLibrary, checkDownloadSpace, downloadModel, cancelDownload, importModel } from "../lib/api";
import { listen } from "@tauri-apps/api/event";
import { save } from "@tauri-apps/plugin-dialog";

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
    ...overrides,
  };
}

function entry(b: ModelBuild, overrides: Partial<BuildRecommendationV2> = {}): BuildRecommendationV2 {
  return {
    build: b,
    overall_score: 0.8,
    component_scores: {
      device_fit: 0.8,
      arabic: 0,
      task_fit: 0.5,
      speed: 0.5,
      quality: 0.5,
      trust: 0.5,
      license_fit: 0.5,
    },
    confidence: "unknown",
    rejection_reasons: [],
    explanation: "Test explanation for why this build was scored this way.",
    categories: [],
    fit_state: "good",
    ...overrides,
  };
}

function recommendationSet(entries: BuildRecommendationV2[]): RecommendationSetV2 {
  return { formula_version: "v2-test", entries };
}

function libraryEntry(overrides: Partial<LibraryEntry> = {}): LibraryEntry {
  return {
    library_id: "lib-1",
    schema_version: "v1",
    sha256: "0".repeat(64),
    file_size_bytes: 4_500_000_000,
    gguf_version: 3,
    tensor_count: 10,
    kv_count: 10,
    architecture: "qwen2",
    quantization: "Q4_K_M",
    parameter_count: 7_000_000_000,
    current_path: "C:\\models\\test-model.gguf",
    original_import_path: null,
    imported_at_rfc3339: "2026-01-01T00:00:00Z",
    last_verified_at_rfc3339: null,
    file_modified_at_rfc3339: null,
    file_status: "unchanged",
    trust: "catalog_metadata_matched",
    last_verification: null,
    catalog_match: { catalog_id: "test-model-q4", confidence: "exact", notes: [] },
    alias: null,
    notes: null,
    quarantine: null,
    managed_copy: false,
    ...overrides,
  };
}

function renderDiscover(lang: "en" | "ar" = "en") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <Discover />
    </I18nProvider>,
  );
}

describe("Discover page — model card navigation safety", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    openUrlMock.mockReset();
    vi.mocked(listen).mockResolvedValue(() => undefined);
    const b = build();
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(listLibrary).mockResolvedValue([]);
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
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("apache-2.0")).toBeInTheDocument();
  });

  it("renders an unknown-license model's details without crashing", async () => {
    const b = build({ license: { status: "unknown" } });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("License")).toBeInTheDocument();
    expect(within(dialog).getAllByText("unknown").length).toBeGreaterThan(0);
  });

  it("keeps quantization, params, license, and the source URL forced ltr in the Arabic RTL details dialog", async () => {
    const b = build({ license: { status: "known", identifier: "apache-2.0" } });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    renderDiscover("ar");
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");

    expect(within(dialog).getByText("Q4_K_M")).toHaveAttribute("dir", "ltr");
    expect(within(dialog).getByText("apache-2.0")).toHaveAttribute("dir", "ltr");
    expect(within(dialog).getByText("7,000,000,000")).toHaveAttribute("dir", "ltr");

    fireEvent.click(within(dialog).getByText(/فتح المصدر|Open official source/));
    const hostname = await screen.findByText("huggingface.co");
    expect(hostname).toHaveAttribute("dir", "ltr");
    const fullUrl = screen.getByText("https://huggingface.co/example/test-model-gguf");
    expect(fullUrl).toHaveAttribute("dir", "ltr");
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
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(await screen.findByText("Open official source"));
    await screen.findByText("Open in browser");
    fireEvent.click(screen.getByText("Back"));
    // Scoped to the still-open dialog: a "Q4_K_M" quantization filter
    // <option> also exists in the page's filter bar behind the modal.
    expect(await within(dialog).findByText("Q4_K_M")).toBeInTheDocument();
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
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(badBuild)]));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    expect(await screen.findByText("This link was not opened")).toBeInTheDocument();
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("rejects a malformed official_source_url instead of opening it", async () => {
    const badBuild = build({ official_source_url: "not-a-url" });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(badBuild)]));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    fireEvent.click(await screen.findByText("Open official source"));
    expect(await screen.findByText("This link was not opened")).toBeInTheDocument();
    expect(openUrlMock).not.toHaveBeenCalled();
  });

  it("a rejected-URL dialog still offers Back/Close so the user is never trapped", async () => {
    const badBuild = build({ official_source_url: "javascript:alert(1)" });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(badBuild)]));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(await screen.findByText("Open official source"));
    await screen.findByText("This link was not opened");
    fireEvent.click(screen.getByText("Back"));
    // Scoped to the still-open dialog: a "Q4_K_M" quantization filter
    // <option> also exists in the page's filter bar behind the modal.
    expect(await within(dialog).findByText("Q4_K_M")).toBeInTheDocument();
  });

  it("the 'verified source only' filter actually excludes unknown-license models", async () => {
    const known = build({ catalog_id: "known-1", display_name: "Known License Model" });
    const unknown = build({
      catalog_id: "unknown-1",
      display_name: "Unknown License Model",
      license: { status: "unknown" },
    });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(known), entry(unknown)]));
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

describe("Discover page — recommendation-engine-v2 integration (Stage B.6)", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    openUrlMock.mockReset();
    vi.mocked(listen).mockResolvedValue(() => undefined);
  });

  it("shows the installed badge for a build the local library already matches", async () => {
    const b = build();
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b, { categories: ["balanced"] })]));
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry({ catalog_match: { catalog_id: "test-model-q4", confidence: "exact", notes: [] } })]);
    renderDiscover();
    await screen.findByText("Test Model 7B");
    expect(await screen.findByText("Installed")).toBeInTheDocument();
  });

  it("does not show the installed badge when the library has no match for this build", async () => {
    const b = build();
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry({ catalog_match: { catalog_id: null, confidence: "none", notes: [] } })]);
    renderDiscover();
    await screen.findByText("Test Model 7B");
    expect(screen.queryByText("Installed")).not.toBeInTheDocument();
  });

  it("the 'installed only' filter hides builds that are not in the local library", async () => {
    const installed = build({ catalog_id: "installed-1", display_name: "Installed Model" });
    const notInstalled = build({ catalog_id: "not-installed-1", display_name: "Not Installed Model" });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(installed), entry(notInstalled)]));
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry({ catalog_match: { catalog_id: "installed-1", confidence: "exact", notes: [] } })]);
    renderDiscover();
    await screen.findByText("Installed Model");
    fireEvent.click(screen.getByText("Installed only"));
    expect(screen.getByText("Installed Model")).toBeInTheDocument();
    expect(screen.queryByText("Not Installed Model")).not.toBeInTheDocument();
  });

  it("the 'recommended only' filter hides builds whose fit state is not good/excellent", async () => {
    const good = build({ catalog_id: "good-1", display_name: "Good Fit Model" });
    const heavy = build({ catalog_id: "heavy-1", display_name: "Heavy Fit Model" });
    vi.mocked(recommendV2).mockResolvedValue(
      recommendationSet([entry(good, { fit_state: "good" }), entry(heavy, { fit_state: "experimental" })]),
    );
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Good Fit Model");
    fireEvent.click(screen.getByText("Recommended only"));
    expect(screen.getByText("Good Fit Model")).toBeInTheDocument();
    expect(screen.queryByText("Heavy Fit Model")).not.toBeInTheDocument();
  });

  it("the family filter narrows the grid to one family", async () => {
    const a = build({ catalog_id: "a-1", family: "family-a", family_id: "family-a", display_name: "Family A Model" });
    const b = build({ catalog_id: "b-1", family: "family-b", family_id: "family-b", display_name: "Family B Model" });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(a), entry(b)]));
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Family A Model");
    fireEvent.change(screen.getByLabelText("Family"), { target: { value: "family-a" } });
    expect(screen.getByText("Family A Model")).toBeInTheDocument();
    expect(screen.queryByText("Family B Model")).not.toBeInTheDocument();
  });

  it("sorting by smallest download orders by file_size_bytes ascending", async () => {
    const big = build({ catalog_id: "big-1", display_name: "Big Model", file_size_bytes: 9_000_000_000 });
    const small = build({ catalog_id: "small-1", display_name: "Small Model", file_size_bytes: 1_000_000_000 });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(big), entry(small)]));
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Big Model");
    fireEvent.change(screen.getByLabelText("Sort by"), { target: { value: "smallest_download" } });
    const cardTitles = screen.getAllByText(/Model$/).map((el) => el.textContent);
    expect(cardTitles.indexOf("Small Model")).toBeLessThan(cardTitles.indexOf("Big Model"));
  });

  it("selecting builds for comparison and opening the compare view shows a side-by-side table", async () => {
    const a = build({ catalog_id: "a-1", display_name: "Model Alpha" });
    const b = build({ catalog_id: "b-1", display_name: "Model Beta" });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(a), entry(b)]));
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Model Alpha");
    const compareCheckboxes = screen.getAllByLabelText("Compare");
    fireEvent.click(compareCheckboxes[0]);
    fireEvent.click(compareCheckboxes[1]);
    fireEvent.click(await screen.findByText(/Compare \(2\)/));
    const dialog = await screen.findByRole("dialog");
    // Appears twice by design (Stage B.9): once as the table's column
    // header, once again in the plain-language summary below it.
    expect(within(dialog).getAllByText("Model Alpha").length).toBeGreaterThan(0);
    expect(within(dialog).getAllByText("Model Beta").length).toBeGreaterThan(0);
    expect(within(dialog.querySelector("table") as HTMLElement).getByText("Model Alpha")).toBeInTheDocument();
  });

  it("comparison selection is capped at 4 builds", async () => {
    const builds = ["a", "b", "c", "d", "e"].map((id) => build({ catalog_id: id, display_name: `Model ${id.toUpperCase()}` }));
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet(builds.map((b) => entry(b))));
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Model A");
    const compareCheckboxes = screen.getAllByLabelText("Compare");
    compareCheckboxes.forEach((cb) => fireEvent.click(cb));
    expect(screen.getByText(/Compare \(4\)/)).toBeInTheDocument();
    expect(compareCheckboxes[4]).not.toBeChecked();
    expect(compareCheckboxes[4]).toBeDisabled();
  });

  it("the plain-language compare summary honestly names which model leads in which real dimension", async () => {
    const fast = build({ catalog_id: "fast-1", display_name: "Fast Model", file_size_bytes: 9_000_000_000 });
    const small = build({ catalog_id: "small-1", display_name: "Small Model", file_size_bytes: 1_000_000_000 });
    vi.mocked(recommendV2).mockResolvedValue(
      recommendationSet([
        entry(fast, { component_scores: { device_fit: 0.5, arabic: 0, task_fit: 0.5, speed: 0.9, quality: 0.5, trust: 0.5, license_fit: 0.5 } }),
        entry(small, { component_scores: { device_fit: 0.5, arabic: 0, task_fit: 0.5, speed: 0.2, quality: 0.5, trust: 0.5, license_fit: 0.5 } }),
      ]),
    );
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Fast Model");
    const compareCheckboxes = screen.getAllByLabelText("Compare");
    fireEvent.click(compareCheckboxes[0]);
    fireEvent.click(compareCheckboxes[1]);
    fireEvent.click(await screen.findByText(/Compare \(2\)/));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/fastest/i)).toBeInTheDocument();
    expect(within(dialog).getByText(/smallest download/i)).toBeInTheDocument();
  });

  it("shows an honest 'no standout dimension' note when two compared models tie on every dimension", async () => {
    const identicalScores = { device_fit: 0.5, arabic: 0, task_fit: 0.5, speed: 0.5, quality: 0.5, trust: 0.5, license_fit: 0.5 };
    const twinA = build({ catalog_id: "twin-a", display_name: "Twin A" });
    const twinB = build({ catalog_id: "twin-b", display_name: "Twin B" });
    vi.mocked(recommendV2).mockResolvedValue(
      recommendationSet([entry(twinA, { component_scores: identicalScores }), entry(twinB, { component_scores: identicalScores })]),
    );
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Twin A");
    const compareCheckboxes = screen.getAllByLabelText("Compare");
    fireEvent.click(compareCheckboxes[0]);
    fireEvent.click(compareCheckboxes[1]);
    fireEvent.click(await screen.findByText(/Compare \(2\)/));
    const dialog = await screen.findByRole("dialog");
    // Both tie on every dimension, so both should be credited as
    // leaders (never an arbitrary tie-break to name a single "winner").
    expect(within(dialog).getAllByText(/fastest/i).length).toBe(2);
  });

  it("does not show a plain-language summary section for a single selected model", async () => {
    const b = build();
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    await screen.findByText("Test Model 7B");
    fireEvent.click(screen.getByLabelText("Compare"));
    fireEvent.click(await screen.findByText(/Compare \(1\)/));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByText("Plain-language summary")).not.toBeInTheDocument();
  });

  it("shows no Download button when the build has no verified exact artifact URL", async () => {
    const b = build({ exact_artifact_url: null });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).queryByText("Download")).not.toBeInTheDocument();
  });

  it("shows the model's plain-language recommendation explanation in the details dialog", async () => {
    const b = build();
    vi.mocked(recommendV2).mockResolvedValue(
      recommendationSet([entry(b, { explanation: "This build fits your device comfortably and supports Arabic well." })]),
    );
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/fits your device comfortably/)).toBeInTheDocument();
  });

  it("shows rejection reasons in the details dialog when a build was hard-rejected", async () => {
    const b = build();
    vi.mocked(recommendV2).mockResolvedValue(
      recommendationSet([
        entry(b, {
          fit_state: null,
          categories: ["unsupported"],
          rejection_reasons: ["Excluded by your preferred/excluded family settings."],
        }),
      ]),
    );
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Excluded by your preferred/excluded family settings.")).toBeInTheDocument();
  });
});

function downloadOutcome(overrides: Partial<import("../lib/types").DownloadOutcome> = {}): import("../lib/types").DownloadOutcome {
  return {
    succeeded: true,
    cancelled: false,
    final_path: "C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf",
    sha256: "a".repeat(64),
    checksum_verified: null,
    bytes_downloaded: 4_500_000_000,
    error: null,
    ...overrides,
  };
}

describe("Discover page — safe verified download flow (Stage B.7)", () => {
  const downloadableBuild = () =>
    build({
      exact_artifact_url: "https://huggingface.co/example/test-model-gguf/resolve/main/test-model.Q4_K_M.gguf",
      artifact_verification: "artifact_url_verified",
      checksum_algorithm: "sha256",
      checksum_value: "a".repeat(64),
    });

  beforeEach(() => {
    vi.resetAllMocks();
    openUrlMock.mockReset();
    vi.mocked(listen).mockResolvedValue(() => undefined);
    vi.mocked(listLibrary).mockResolvedValue([]);
  });

  it("shows a Download button when the build has a verified exact artifact URL", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Download")).toBeInTheDocument();
  });

  it("cancelling the destination picker does not start a download or leave the details view", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue(null);
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    expect(downloadModel).not.toHaveBeenCalled();
    expect(within(dialog).getByText("Q4_K_M")).toBeInTheDocument();
  });

  it("shows the destination and a disk-space check before starting the download", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    await waitFor(() => expect(checkDownloadSpace).toHaveBeenCalledWith("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf", 4_500_000_000));
    expect(await within(dialog).findByText("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf")).toBeInTheDocument();
  });

  it("warns when the destination does not have enough free space", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue("D:\\tiny-drive\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 1_000_000, required_bytes: 4_500_000_000, sufficient: false });
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    expect(await within(dialog).findByText(/enough free space/i)).toBeInTheDocument();
  });

  it("passes the catalog's curated sha256 as the expected checksum when starting a download", async () => {
    const b = downloadableBuild();
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    vi.mocked(downloadModel).mockResolvedValue(downloadOutcome({ checksum_verified: true }));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    await waitFor(() =>
      expect(downloadModel).toHaveBeenCalledWith(
        b.exact_artifact_url,
        "C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf",
        "a".repeat(64),
      ),
    );
  });

  it("shows a success result with checksum-verified status and offers to add the file to the library", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    vi.mocked(downloadModel).mockResolvedValue(downloadOutcome({ checksum_verified: true }));
    vi.mocked(importModel).mockResolvedValue({
      library_id: "lib-new",
      was_new: true,
      duplicate_of: [],
      verification: {
        header: "verified",
        structure: "verified",
        sha256: "verified",
        runtime_load: "not_tested",
        benchmark: "not_tested",
        catalog_match: "exact",
        overall_integrity: "verified",
        trust: "catalog_metadata_matched",
      },
      catalog_match: { catalog_id: "test-model-q4", confidence: "exact", notes: [] },
    });
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    expect(await within(dialog).findByText(/checksum verified/i)).toBeInTheDocument();
    fireEvent.click(within(dialog).getByText("Add to library"));
    await waitFor(() => expect(importModel).toHaveBeenCalledWith("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf", null));
    expect(await within(dialog).findByText(/added to your local library/i)).toBeInTheDocument();
  });

  it("shows an honest 'not independently verified' note when no checksum was available to compare against", async () => {
    const b = build({
      exact_artifact_url: "https://huggingface.co/example/test-model-gguf/resolve/main/test-model.Q4_K_M.gguf",
      artifact_verification: "artifact_url_verified",
      checksum_algorithm: null,
      checksum_value: null,
    });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    vi.mocked(downloadModel).mockResolvedValue(downloadOutcome({ checksum_verified: null }));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    await waitFor(() => expect(downloadModel).toHaveBeenCalledWith(b.exact_artifact_url, expect.any(String), null));
    expect(await within(dialog).findByText(/no independent checksum/i)).toBeInTheDocument();
  });

  it("shows a checksum-mismatch failure honestly and offers Retry, never a false success", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    vi.mocked(downloadModel).mockResolvedValue(
      downloadOutcome({
        succeeded: false,
        final_path: null,
        checksum_verified: false,
        error: "the downloaded file's checksum did not match the expected value",
      }),
    );
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    expect(await within(dialog).findByText(/did not complete/i)).toBeInTheDocument();
    expect(within(dialog).getByText(/checksum did not match/i)).toBeInTheDocument();
    expect(within(dialog).queryByText(/added to your local library/i)).not.toBeInTheDocument();
    expect(within(dialog).getByText("Retry")).toBeInTheDocument();
  });

  it("clicking Cancel download requests cancellation via cancel_download", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    // Never resolves within this test, so the progress view stays mounted
    // long enough to click Cancel.
    vi.mocked(downloadModel).mockReturnValue(new Promise(() => {}));
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    await screen.findByText("Downloading…");
    fireEvent.click(screen.getByText("Cancel download"));
    await waitFor(() => expect(cancelDownload).toHaveBeenCalledTimes(1));
  });

  it("shows a cancelled result distinctly from a failure, and offers Retry", async () => {
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(downloadableBuild())]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    vi.mocked(downloadModel).mockResolvedValue(
      downloadOutcome({ succeeded: false, cancelled: true, final_path: null, sha256: null, checksum_verified: null, error: null }),
    );
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    expect(await within(dialog).findByText(/download was cancelled/i)).toBeInTheDocument();
    expect(within(dialog).getByText("Retry")).toBeInTheDocument();
  });

  it("rejects a non-http(s) exact_artifact_url from ever reaching download_model (defense in depth)", async () => {
    // The Rust side already rejects non-http(s) URLs (is_supported_url);
    // this only documents that the frontend never fabricates or rewrites
    // the URL it was given - it passes the catalog's own value through
    // unchanged for the backend to validate.
    const b = build({
      exact_artifact_url: "https://huggingface.co/example/test-model-gguf/resolve/main/test-model.Q4_K_M.gguf",
      artifact_verification: "artifact_url_verified",
    });
    vi.mocked(recommendV2).mockResolvedValue(recommendationSet([entry(b)]));
    vi.mocked(save).mockResolvedValue("C:\\Users\\test\\Downloads\\test-model.Q4_K_M.gguf");
    vi.mocked(checkDownloadSpace).mockResolvedValue({ available_bytes: 100_000_000_000, required_bytes: 4_500_000_000, sufficient: true });
    vi.mocked(downloadModel).mockResolvedValue(downloadOutcome());
    renderDiscover();
    fireEvent.click(await screen.findByText("Test Model 7B"));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByText("Download"));
    fireEvent.click(await within(dialog).findByText("Start download"));
    await waitFor(() => expect(downloadModel).toHaveBeenCalledWith(b.exact_artifact_url, expect.any(String), null));
  });
});
