import { describe, expect, it, vi, beforeEach } from "vitest";
import { render, screen } from "@testing-library/react";
import { I18nProvider } from "../i18n/I18nContext";
import { Health } from "./Health";
import type { AuditReport, LibraryEntry } from "../lib/types";

vi.mock("../lib/api", () => ({
  auditLibrary: vi.fn(),
  listLibrary: vi.fn(),
  exportLibrary: vi.fn(),
  unquarantineModel: vi.fn(),
}));

import { auditLibrary, listLibrary } from "../lib/api";

const HASH = "a".repeat(64);

function auditReport(): AuditReport {
  return {
    healthy: ["model-instance-12ddb13fc87b9f539bd5d87baab1c840"],
    missing: [],
    modified: [],
    corrupt: [],
    duplicate_groups: [
      {
        sha256: HASH,
        members: [
          { library_id: "model-instance-aaa", current_path: "C:\\Models\\a.gguf" },
          { library_id: "model-instance-bbb", current_path: "C:\\Models\\b.gguf" },
        ],
      },
    ],
    stale_profiles: [{ library_id: "model-instance-stale", detail: "Profile references a missing model." }],
    stale_calibrations: [],
    unknown_provenance: [],
    unsupported: [],
    privacy_concerns: [{ library_id: "model-instance-priv", field: "original_import_path", detail: "Retains the original import path." }],
    schema_migration_needed: [],
    recommendations: [],
  };
}

function libraryEntry(): LibraryEntry {
  return {
    library_id: "model-instance-quarantined",
    schema_version: "v1",
    sha256: "b".repeat(64),
    file_size_bytes: 500_000_000,
    gguf_version: 3,
    tensor_count: 10,
    kv_count: 5,
    architecture: "qwen2",
    quantization: "Q4_K_M",
    parameter_count: 500_000_000,
    current_path: "C:\\Models\\quarantined.gguf",
    original_import_path: "C:\\Models\\quarantined.gguf",
    imported_at_rfc3339: "2026-01-01T00:00:00Z",
    last_verified_at_rfc3339: null,
    file_modified_at_rfc3339: null,
    file_status: "unchanged",
    trust: "local_unverified_source",
    last_verification: null,
    catalog_match: { catalog_id: null, confidence: "none", notes: [] },
    alias: null,
    notes: null,
    quarantine: { reason: "manual", quarantined_at_rfc3339: "2026-01-01T00:00:00Z" },
    managed_copy: false,
  };
}

function renderHealth(lang: "en" | "ar" = "en") {
  window.localStorage.setItem("brute.language", lang);
  return render(
    <I18nProvider>
      <Health />
    </I18nProvider>,
  );
}

describe("Health page", () => {
  beforeEach(() => {
    vi.mocked(auditLibrary).mockResolvedValue(auditReport());
    vi.mocked(listLibrary).mockResolvedValue([libraryEntry()]);
  });

  it("keeps model IDs, the SHA-256 hash, and quarantined identifiers forced ltr in the Arabic RTL page", async () => {
    renderHealth("ar");

    const healthyId = await screen.findByText("model-instance-12ddb13fc87b9f539bd5d87baab1c840");
    expect(healthyId).toHaveAttribute("dir", "ltr");

    const staleId = screen.getByText("model-instance-stale");
    expect(staleId).toHaveAttribute("dir", "ltr");

    const privacyId = screen.getByText("model-instance-priv");
    expect(privacyId).toHaveAttribute("dir", "ltr");

    const hashPrefix = screen.getByText(`${HASH.slice(0, 12)}…`);
    expect(hashPrefix).toHaveAttribute("dir", "ltr");
    expect(hashPrefix).toHaveAttribute("title", HASH);

    const quarantinedName = screen.getByText("quarantined.gguf");
    expect(quarantinedName).toHaveAttribute("dir", "ltr");
  });
});
