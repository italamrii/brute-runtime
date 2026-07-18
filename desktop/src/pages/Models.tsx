import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import {
  forgetModel,
  getAssociations,
  importDirectory,
  importModel,
  listLibrary,
  quarantineModel,
  scanDirectory,
  setAlias,
  setNote,
  unquarantineModel,
  verifyLibraryEntries,
} from "../lib/api";
import type { AssociationsDto, LibraryEntry, ScanResult } from "../lib/types";
import { formatBytes, formatDate, shortHash } from "../lib/format";
import { useAppStatus } from "../lib/AppStatusContext";

function trustTone(entry: LibraryEntry): "good" | "warn" | "bad" | "unknown" {
  if (entry.quarantine) return "bad";
  switch (entry.trust) {
    case "catalog_metadata_matched":
      return "good";
    case "local_unverified_source":
      return "unknown";
    case "modified_since_verification":
    case "corrupt":
    case "missing":
      return "bad";
    case "unsupported":
      return "warn";
    default:
      return "unknown";
  }
}

export function Models() {
  const { t } = useI18n();
  const status = useAppStatus();
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [associations, setAssociations] = useState<AssociationsDto | null>(null);
  const [scanPreview, setScanPreview] = useState<{ root: string; result: ScanResult } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  function refresh() {
    listLibrary()
      .then(setEntries)
      .catch((e) => setError(String(e)));
  }

  useEffect(refresh, []);

  useEffect(() => {
    if (!selectedId) {
      setAssociations(null);
      return;
    }
    getAssociations(selectedId).then(setAssociations).catch(() => setAssociations(null));
  }, [selectedId]);

  const filtered = useMemo(() => {
    if (!entries) return [];
    const q = search.trim().toLowerCase();
    if (!q) return entries;
    return entries.filter((e) =>
      [e.alias, e.architecture, e.quantization, e.current_path, e.library_id].some((f) => f?.toLowerCase().includes(q)),
    );
  }, [entries, search]);

  const selected = entries?.find((e) => e.library_id === selectedId) ?? null;

  async function handleImport() {
    setError(null);
    setNotice(null);
    const path = await open({ multiple: false, filters: [{ name: "GGUF model", extensions: ["gguf"] }] });
    if (typeof path !== "string") return;
    setBusy(true);
    try {
      const outcome = await importModel(path, null);
      setNotice(outcome.was_new ? "Model imported." : "Model already tracked — re-verified.");
      refresh();
      setSelectedId(outcome.library_id);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleChooseScanRoot() {
    setError(null);
    setScanPreview(null);
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir !== "string") return;
    setBusy(true);
    try {
      const result = await scanDirectory(dir, { recursive: true, max_depth: 8, max_files: 5000 });
      setScanPreview({ root: dir, result });
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleConfirmScanImport() {
    if (!scanPreview) return;
    setBusy(true);
    setError(null);
    try {
      const outcome = await importDirectory(scanPreview.root, { recursive: true, max_depth: 8, max_files: 5000 });
      const importedCount = outcome.imported.filter((x) => "Ok" in x[1]).length;
      setNotice(`Imported ${importedCount} of ${outcome.imported.length} discovered candidate(s).`);
      setScanPreview(null);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleVerify(id: string) {
    setBusy(true);
    try {
      await verifyLibraryEntries(id, false);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleForget(id: string) {
    setBusy(true);
    try {
      await forgetModel(id);
      setSelectedId(null);
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleQuarantineToggle(entry: LibraryEntry) {
    setBusy(true);
    try {
      if (entry.quarantine) {
        await unquarantineModel(entry.library_id);
      } else {
        await quarantineModel(entry.library_id, "Quarantined manually from the Models page.");
      }
      refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1 className="page-title">{t("models_title")}</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn btn-primary" onClick={handleImport} disabled={busy}>
            {t("models_import")}
          </button>
          <button className="btn" onClick={handleChooseScanRoot} disabled={busy}>
            {t("models_scan")}
          </button>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {notice && (
        <div className="panel" style={{ marginBottom: 16, borderColor: "var(--state-good)" }}>
          {notice}
        </div>
      )}

      {scanPreview && (
        <div className="panel" style={{ marginBottom: 16 }}>
          <div className="panel-title">Scan preview — {scanPreview.root}</div>
          <p className="text-secondary">
            {scanPreview.result.discovered.filter((d) => d.kind === "gguf_candidate").length} GGUF candidate(s),{" "}
            {scanPreview.result.discovered.filter((d) => d.kind === "unsupported_format").length} unsupported,{" "}
            {scanPreview.result.discovered.filter((d) => d.kind === "inaccessible").length} inaccessible.
            {scanPreview.result.truncated_by_file_count && " (truncated by file limit)"}
          </p>
          <div style={{ display: "flex", gap: 8, marginTop: 8 }}>
            <button className="btn btn-primary" onClick={handleConfirmScanImport} disabled={busy}>
              Import discovered candidates
            </button>
            <button className="btn" onClick={() => setScanPreview(null)} disabled={busy}>
              {t("common_cancel")}
            </button>
          </div>
        </div>
      )}

      <div className="field" style={{ maxWidth: 320 }}>
        <input
          type="text"
          placeholder={t("common_search")}
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          aria-label={t("common_search")}
        />
      </div>

      {entries && entries.length === 0 && <div className="empty-state">{t("models_empty")}</div>}

      {entries && entries.length > 0 && (
        <div style={{ display: "grid", gridTemplateColumns: selected ? "1fr 380px" : "1fr", gap: 16 }}>
          <div className="panel" style={{ overflowX: "auto" }}>
            <table className="data-table">
              <thead>
                <tr>
                  <th>Name</th>
                  <th>Architecture</th>
                  <th>Quant</th>
                  <th>Size</th>
                  <th>Status</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((e) => (
                  <tr
                    key={e.library_id}
                    onClick={() => setSelectedId(e.library_id)}
                    style={{ cursor: "pointer", background: e.library_id === selectedId ? "var(--bg-panel-raised)" : undefined }}
                  >
                    <td>{e.alias ?? e.current_path.split(/[\\/]/).pop()}</td>
                    <td>{e.architecture ?? "—"}</td>
                    <td>{e.quantization ?? "—"}</td>
                    <td>{formatBytes(e.file_size_bytes)}</td>
                    <td>
                      <span className={`badge badge-${trustTone(e)}`}>{e.quarantine ? t("models_quarantined_badge") : e.trust}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          {selected && (
            <div className="panel">
              <div className="panel-title">{t("common_details")}</div>
              <h3 style={{ marginBottom: 8 }}>{selected.alias ?? selected.current_path.split(/[\\/]/).pop()}</h3>
              <div className="text-tertiary mono" style={{ wordBreak: "break-all", marginBottom: 8 }}>
                {selected.current_path}
              </div>

              <dl style={{ margin: 0 }}>
                {[
                  ["Architecture", selected.architecture ?? "—"],
                  ["Quantization", selected.quantization ?? "—"],
                  ["Parameters", selected.parameter_count ? selected.parameter_count.toLocaleString() : "—"],
                  ["Size", formatBytes(selected.file_size_bytes)],
                  ["SHA-256", shortHash(selected.sha256, 20)],
                  ["Integrity", selected.last_verification?.overall_integrity ?? "—"],
                  ["Trust", selected.trust],
                  ["Catalog match", selected.catalog_match.confidence],
                  ["Imported", formatDate(selected.imported_at_rfc3339)],
                  ["Last verified", formatDate(selected.last_verified_at_rfc3339)],
                ].map(([label, value]) => (
                  <div key={label} style={{ display: "flex", justifyContent: "space-between", padding: "4px 0", borderBottom: "1px solid var(--border-subtle)" }}>
                    <span className="text-secondary">{label}</span>
                    <span className="mono">{value}</span>
                  </div>
                ))}
              </dl>

              {selected.catalog_match.confidence !== "none" && (
                <p className="text-tertiary" style={{ marginTop: 8 }}>
                  Review the official license before deployment.
                </p>
              )}

              {associations && (
                <p className="text-secondary" style={{ marginTop: 8 }}>
                  {associations.runtime_profiles.length} saved profile(s) · {associations.calibration_record_count} calibration match(es)
                </p>
              )}

              <div style={{ display: "flex", gap: 8, flexWrap: "wrap", marginTop: 12 }}>
                <button className="btn" onClick={() => handleVerify(selected.library_id)} disabled={busy}>
                  {t("common_verify")}
                </button>
                <button
                  className="btn"
                  onClick={() => {
                    status.setActiveModel(selected.library_id, selected.alias ?? selected.current_path.split(/[\\/]/).pop() ?? null);
                    setNotice("Set as active model for the Run workspace.");
                  }}
                  disabled={busy}
                >
                  Use in Run
                </button>
                <button className="btn" onClick={() => handleQuarantineToggle(selected)} disabled={busy}>
                  {selected.quarantine ? "Unquarantine" : "Quarantine"}
                </button>
                <button
                  className="btn"
                  onClick={async () => {
                    const name = window.prompt("Alias for this model:", selected.alias ?? "");
                    if (name === null) return;
                    setBusy(true);
                    try {
                      await setAlias(selected.library_id, name);
                      refresh();
                    } catch (e) {
                      setError(String(e));
                    } finally {
                      setBusy(false);
                    }
                  }}
                  disabled={busy}
                >
                  Set alias
                </button>
                <button
                  className="btn"
                  onClick={async () => {
                    const text = window.prompt("Notes for this model:", selected.notes ?? "");
                    if (text === null) return;
                    setBusy(true);
                    try {
                      await setNote(selected.library_id, text);
                      refresh();
                    } catch (e) {
                      setError(String(e));
                    } finally {
                      setBusy(false);
                    }
                  }}
                  disabled={busy}
                >
                  Set notes
                </button>
                <button className="btn btn-danger" onClick={() => handleForget(selected.library_id)} disabled={busy}>
                  Remove from library
                </button>
              </div>
              <p className="text-tertiary" style={{ marginTop: 8, fontSize: 11 }}>
                {t("models_forget_label")}
              </p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
