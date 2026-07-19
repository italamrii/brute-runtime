import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import {
  forgetModel,
  getAssociations,
  importDirectory,
  importModel,
  listLibrary,
  locateModel,
  quarantineModel,
  scanDirectory,
  setAlias,
  setNote,
  unquarantineModel,
  verifyLibraryEntries,
} from "../lib/api";
import type { AssociationsDto, LibraryEntry, ScanResult, TrustStatus } from "../lib/types";
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

type TrustFilter = "all" | TrustStatus | "quarantined";

export function Models() {
  const { t } = useI18n();
  const status = useAppStatus();
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [trustFilter, setTrustFilter] = useState<TrustFilter>("all");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [associations, setAssociations] = useState<AssociationsDto | null>(null);
  const [scanPreview, setScanPreview] = useState<{ root: string; result: ScanResult } | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [showTechnical, setShowTechnical] = useState(false);

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
    return entries.filter((e) => {
      if (trustFilter === "quarantined" && !e.quarantine) return false;
      if (trustFilter !== "all" && trustFilter !== "quarantined" && e.trust !== trustFilter) return false;
      if (!q) return true;
      return [e.alias, e.architecture, e.quantization, e.current_path, e.library_id, e.sha256].some((f) =>
        f?.toLowerCase().includes(q),
      );
    });
  }, [entries, search, trustFilter]);

  const selected = entries?.find((e) => e.library_id === selectedId) ?? null;

  async function handleImport() {
    setError(null);
    setNotice(null);
    const path = await open({ multiple: false, filters: [{ name: "GGUF model", extensions: ["gguf"] }] });
    if (typeof path !== "string") return;
    setBusy(true);
    try {
      const outcome = await importModel(path, null);
      setNotice(outcome.was_new ? t("models_imported") : t("models_reverified"));
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
      setNotice(t("models_scan_imported").replace("{n}", String(importedCount)).replace("{total}", String(outcome.imported.length)));
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

  async function handleLocate(entry: LibraryEntry) {
    const path = await open({ multiple: false, filters: [{ name: "GGUF model", extensions: ["gguf"] }] });
    if (typeof path !== "string") return;
    setBusy(true);
    try {
      await locateModel(entry.library_id, path);
      setNotice(t("models_located"));
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
        <div>
          <div className="page-kicker">{t("models_kicker")}</div>
          <h1 className="page-title">{t("models_title")}</h1>
          <p className="page-desc">{t("models_desc")}</p>
        </div>
        <div className="page-actions">
          <button className="btn btn-primary" onClick={handleImport} disabled={busy} type="button">
            {t("models_import")}
          </button>
          <button className="btn" onClick={handleChooseScanRoot} disabled={busy} type="button">
            {t("models_scan")}
          </button>
        </div>
      </div>

      {error && (
        <div className="error-banner">
          <div>{error}</div>
          <details className="details-toggle">
            <summary>{t("common_technical_details")}</summary>
            <pre>{error}</pre>
          </details>
        </div>
      )}
      {notice && <div className="notice-banner">{notice}</div>}

      {scanPreview && (
        <div className="panel" style={{ marginBottom: 16 }}>
          <div className="panel-title">{t("models_scan_preview")}</div>
          <p className="path-text text-secondary" style={{ marginBottom: 8 }}>
            {scanPreview.root}
          </p>
          <p className="text-secondary">
            {scanPreview.result.discovered.filter((d) => d.kind === "gguf_candidate").length} GGUF ·{" "}
            {scanPreview.result.discovered.filter((d) => d.kind === "unsupported_format").length} unsupported ·{" "}
            {scanPreview.result.discovered.filter((d) => d.kind === "inaccessible").length} inaccessible
            {scanPreview.result.truncated_by_file_count ? ` · ${t("models_scan_truncated")}` : ""}
          </p>
          <div className="page-actions" style={{ marginTop: 10 }}>
            <button className="btn btn-primary" onClick={handleConfirmScanImport} disabled={busy} type="button">
              {t("models_import_discovered")}
            </button>
            <button className="btn" onClick={() => setScanPreview(null)} disabled={busy} type="button">
              {t("common_cancel")}
            </button>
          </div>
        </div>
      )}

      <div className="filter-bar">
        <div className="field" style={{ flex: 1, minWidth: 200 }}>
          <label htmlFor="models-search">{t("common_search")}</label>
          <input
            id="models-search"
            type="text"
            placeholder={t("common_search")}
            value={search}
            onChange={(e) => setSearch(e.target.value)}
          />
        </div>
        <div className="field">
          <label htmlFor="models-filter">{t("models_filter_trust")}</label>
          <select id="models-filter" value={trustFilter} onChange={(e) => setTrustFilter(e.target.value as TrustFilter)}>
            <option value="all">{t("models_filter_all")}</option>
            <option value="catalog_metadata_matched">{t("models_filter_catalog")}</option>
            <option value="local_unverified_source">{t("models_filter_local")}</option>
            <option value="quarantined">{t("models_quarantined_badge")}</option>
            <option value="corrupt">corrupt</option>
            <option value="missing">missing</option>
            <option value="unsupported">unsupported</option>
          </select>
        </div>
      </div>

      {entries && entries.length === 0 && <div className="empty-state">{t("models_empty")}</div>}

      {entries && entries.length > 0 && (
        <div className={`workspace-split ${selected ? "" : "is-single"}`}>
          <div className="table-scroll">
            <table className="data-table">
              <thead>
                <tr>
                  <th>{t("models_col_name")}</th>
                  <th>{t("models_col_arch")}</th>
                  <th>{t("models_col_quant")}</th>
                  <th>{t("models_col_size")}</th>
                  <th>{t("models_col_status")}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((e) => (
                  <tr
                    key={e.library_id}
                    onClick={() => setSelectedId(e.library_id)}
                    onKeyDown={(ev) => {
                      if (ev.key === "Enter" || ev.key === " ") {
                        ev.preventDefault();
                        setSelectedId(e.library_id);
                      }
                    }}
                    tabIndex={0}
                    aria-selected={e.library_id === selectedId}
                  >
                    <td>{e.alias ?? e.current_path.split(/[\\/]/).pop()}</td>
                    <td>{e.architecture ?? "—"}</td>
                    <td className="mono">{e.quantization ?? "—"}</td>
                    <td className="num">{formatBytes(e.file_size_bytes)}</td>
                    <td>
                      <span className={`badge badge-${trustTone(e)}`}>
                        {e.quarantine ? t("models_quarantined_badge") : e.trust}
                      </span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {filtered.length === 0 && <div className="empty-state">{t("models_no_match")}</div>}
          </div>

          {selected && (
            <aside className="inspector" aria-label={t("common_details")}>
              <div className="metric-card-label">{t("common_details")}</div>
              <h3 className="inspector-title">{selected.alias ?? selected.current_path.split(/[\\/]/).pop()}</h3>
              <div className="inspector-path">{selected.current_path}</div>

              <div className="chip-row" style={{ marginBottom: 12 }}>
                <span className={`badge badge-${trustTone(selected)}`}>
                  {selected.quarantine ? t("models_quarantined_badge") : selected.trust}
                </span>
                <span className="badge badge-detected">{selected.catalog_match.confidence}</span>
              </div>

              <div className="kv-row">
                <span className="kv-row-label">{t("models_col_arch")}</span>
                <span className="kv-row-value mono">{selected.architecture ?? "—"}</span>
              </div>
              <div className="kv-row">
                <span className="kv-row-label">{t("models_col_quant")}</span>
                <span className="kv-row-value mono">{selected.quantization ?? "—"}</span>
              </div>
              <div className="kv-row">
                <span className="kv-row-label">{t("models_params")}</span>
                <span className="kv-row-value num">
                  {selected.parameter_count ? selected.parameter_count.toLocaleString() : "—"}
                </span>
              </div>
              <div className="kv-row">
                <span className="kv-row-label">{t("models_col_size")}</span>
                <span className="kv-row-value num">{formatBytes(selected.file_size_bytes)}</span>
              </div>
              <div className="kv-row">
                <span className="kv-row-label">SHA-256</span>
                <span className="kv-row-value mono" title={selected.sha256}>
                  {shortHash(selected.sha256, 20)}
                </span>
              </div>
              <div className="kv-row">
                <span className="kv-row-label">{t("models_integrity")}</span>
                <span className="kv-row-value mono">{selected.last_verification?.overall_integrity ?? "—"}</span>
              </div>
              <div className="kv-row">
                <span className="kv-row-label">{t("models_last_verified")}</span>
                <span className="kv-row-value">{formatDate(selected.last_verified_at_rfc3339)}</span>
              </div>

              {showTechnical && (
                <>
                  <div className="kv-row">
                    <span className="kv-row-label">GGUF</span>
                    <span className="kv-row-value mono">{selected.gguf_version}</span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-row-label">{t("models_tensors")}</span>
                    <span className="kv-row-value num">{selected.tensor_count}</span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-row-label">{t("models_file_status")}</span>
                    <span className="kv-row-value mono">{selected.file_status}</span>
                  </div>
                  <div className="kv-row">
                    <span className="kv-row-label">{t("models_imported_at")}</span>
                    <span className="kv-row-value">{formatDate(selected.imported_at_rfc3339)}</span>
                  </div>
                  {selected.notes && (
                    <p className="text-secondary" style={{ marginTop: 8, fontSize: 12 }}>
                      {selected.notes}
                    </p>
                  )}
                </>
              )}

              <button className="btn btn-ghost btn-sm" type="button" onClick={() => setShowTechnical((v) => !v)} style={{ marginTop: 8 }}>
                {showTechnical ? t("optimize_simple_tab") : t("optimize_technical_tab")}
              </button>

              {selected.catalog_match.confidence !== "none" && (
                <p className="text-tertiary" style={{ marginTop: 8, fontSize: 11 }}>
                  {t("models_license_review")}
                </p>
              )}

              {associations && (
                <p className="text-secondary" style={{ marginTop: 8, fontSize: 12 }}>
                  {associations.runtime_profiles.length} {t("models_profiles_count")} ·{" "}
                  {associations.calibration_record_count} {t("models_calibrations_count")}
                </p>
              )}

              <div className="action-stack" style={{ marginTop: 14 }}>
                <button className="btn btn-primary" type="button" onClick={() => handleVerify(selected.library_id)} disabled={busy}>
                  {t("common_verify")}
                </button>
                <button
                  className="btn"
                  type="button"
                  onClick={() => {
                    status.setActiveModel(selected.library_id, selected.alias ?? selected.current_path.split(/[\\/]/).pop() ?? null);
                    setNotice(t("models_set_active"));
                  }}
                  disabled={busy}
                >
                  {t("models_use_in_run")}
                </button>
                <button className="btn" type="button" onClick={() => handleLocate(selected)} disabled={busy}>
                  {t("models_locate")}
                </button>
                <button className="btn" type="button" onClick={() => handleQuarantineToggle(selected)} disabled={busy}>
                  {selected.quarantine ? t("models_unquarantine") : t("models_quarantine")}
                </button>
                <button
                  className="btn"
                  type="button"
                  onClick={async () => {
                    const name = window.prompt(t("models_alias_prompt"), selected.alias ?? "");
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
                  {t("models_set_alias")}
                </button>
                <button
                  className="btn"
                  type="button"
                  onClick={async () => {
                    const text = window.prompt(t("models_notes_prompt"), selected.notes ?? "");
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
                  {t("models_set_notes")}
                </button>
                <button className="btn btn-danger" type="button" onClick={() => handleForget(selected.library_id)} disabled={busy}>
                  {t("models_forget_action")}
                </button>
              </div>
              <p className="text-tertiary" style={{ marginTop: 8, fontSize: 11 }}>
                {t("models_forget_label")}
              </p>
            </aside>
          )}
        </div>
      )}
    </div>
  );
}
