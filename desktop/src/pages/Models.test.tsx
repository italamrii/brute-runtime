import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { AppStatusProvider } from "../lib/AppStatusContext";
import { Models } from "./Models";
import type { LibraryEntry } from "../lib/types";

vi.mock("../lib/api", () => ({
  listLibrary: vi.fn(),
  getAssociations: vi.fn().mockResolvedValue({ runtime_profiles: [], calibration_record_count: 0 }),
  importModel: vi.fn(),
  scanDirectory: vi.fn(),
  importDirectory: vi.fn(),
  verifyLibraryEntries: vi.fn(),
  forgetModel: vi.fn(),
  quarantineModel: vi.fn(),
  unquarantineModel: vi.fn(),
  setAlias: vi.fn(),
  setNote: vi.fn(),
}));

import { listLibrary } from "../lib/api";

function entry(overrides: Partial<LibraryEntry> = {}): LibraryEntry {
  return {
    library_id: "model-1",
    schema_version: "v1",
    sha256: "a".repeat(64),
    file_size_bytes: 500_000_000,
    gguf_version: 3,
    tensor_count: 10,
    kv_count: 5,
    architecture: "qwen2",
    quantization: "Q4_K_M",
    parameter_count: 500_000_000,
    current_path: "C:\\Models\\test.gguf",
    original_import_path: "C:\\Models\\test.gguf",
    imported_at_rfc3339: "2026-01-01T00:00:00Z",
    last_verified_at_rfc3339: "2026-01-01T00:00:00Z",
    file_modified_at_rfc3339: null,
    file_status: "unchanged",
    trust: "local_unverified_source",
    last_verification: null,
    catalog_match: { catalog_id: null, confidence: "none", notes: [] },
    alias: null,
    notes: null,
    quarantine: null,
    managed_copy: false,
    ...overrides,
  };
}

function renderModels() {
  return render(
    <I18nProvider>
      <AppStatusProvider>
        <Models />
      </AppStatusProvider>
    </I18nProvider>,
  );
}

describe("Models page", () => {
  beforeEach(() => {
    vi.mocked(listLibrary).mockResolvedValue([entry()]);
  });

  it("shows the empty state honestly when the library has no models", async () => {
    vi.mocked(listLibrary).mockResolvedValue([]);
    renderModels();
    expect(await screen.findByText(/no models in the library yet/i)).toBeInTheDocument();
  });

  it("never labels the forget action as deletion — it must say the file stays on disk", async () => {
    renderModels();
    fireEvent.click(await screen.findByText("test.gguf"));
    const removeButton = await screen.findByText("Remove from library");
    expect(removeButton).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^delete$/i })).not.toBeInTheDocument();
    expect(screen.getByText(/model file remains on disk/i)).toBeInTheDocument();
  });

  it("badges a quarantined model distinctly rather than showing its trust state", async () => {
    vi.mocked(listLibrary).mockResolvedValue([
      entry({ quarantine: { reason: "manual", quarantined_at_rfc3339: "2026-01-01T00:00:00Z" } }),
    ]);
    renderModels();
    await waitFor(() => expect(screen.getByText("Quarantined")).toBeInTheDocument());
  });
});
