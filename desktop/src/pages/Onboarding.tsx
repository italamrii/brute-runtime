import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { importModel, scanCommonModelLocations, scanDirectory } from "../lib/api";
import type { DiscoveryResult } from "../lib/types";
import { useState } from "react";
import type { Page } from "../App";

/** First-run screen: one short explanation, no internet/sign-in, fully
 * skippable. "Scan device" scans only a fixed, safe set of common model
 * folders (never the whole disk) — see commands::discovery. */
export function Onboarding({ onDone }: { onDone: (targetPage?: Page) => void }) {
  const { t, lang, setLang } = useI18n();
  const status = useAppStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scanResults, setScanResults] = useState<DiscoveryResult[] | null>(null);
  const [scanned, setScanned] = useState(false);

  async function handleImport() {
    setError(null);
    setBusy(true);
    try {
      const path = await open({
        multiple: false,
        filters: [{ name: "GGUF model", extensions: ["gguf"] }],
      });
      if (typeof path === "string") {
        const outcome = await importModel(path, null);
        status.setActiveModel(outcome.library_id, path.split(/[\\/]/).pop() ?? path);
      }
      onDone();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleScanCommonLocations() {
    setError(null);
    setBusy(true);
    try {
      const results = await scanCommonModelLocations();
      setScanResults(results);
      setScanned(true);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleAddFolder() {
    setError(null);
    setBusy(true);
    try {
      const dir = await open({ directory: true, multiple: false });
      if (typeof dir === "string") {
        const result = await scanDirectory(dir, { recursive: true, max_depth: 8, max_files: 5000 });
        setScanResults([{ label: "common_location_custom", path: dir, scan: result }]);
        setScanned(true);
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const candidates = (scanResults ?? []).flatMap((r) =>
    r.scan.discovered.filter((d) => d.kind === "gguf_candidate").map((d) => ({ ...d, location: r.path })),
  );

  async function handleImportAllDiscovered() {
    setBusy(true);
    setError(null);
    try {
      for (const c of candidates) {
        await importModel(c.path, null);
      }
      onDone("models");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="onboarding-shell">
      <div className="onboarding-card">
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16, gap: 12 }}>
          <div className="sidebar-brand-mark">
            <div className="sidebar-brand-glyph" aria-hidden="true">
              B
            </div>
            <div>
              <div className="sidebar-brand-title">BRUTE</div>
              <div className="sidebar-brand-sub">Runtime</div>
            </div>
          </div>
          <select
            aria-label={t("settings_language")}
            value={lang}
            onChange={(e) => setLang(e.target.value as "en" | "ar")}
            style={{
              background: "var(--bg-inset)",
              color: "var(--text-primary)",
              border: "1px solid var(--border-strong)",
              borderRadius: "var(--radius-sm)",
              padding: "6px 8px",
            }}
          >
            <option value="en">English</option>
            <option value="ar">العربية</option>
          </select>
        </div>

        <h1 className="page-title" style={{ marginBottom: 8 }}>
          {t("onboarding_title")}
        </h1>
        <p className="text-secondary" style={{ marginBottom: 20 }}>
          {t("onboarding_body")}
        </p>

        <ul className="onboarding-points">
          <li>{t("onboarding_point_local")}</li>
          <li>{t("onboarding_point_no_account")}</li>
          <li>{t("onboarding_point_no_upload")}</li>
          <li>{t("onboarding_point_control")}</li>
        </ul>

        {error && <div className="error-banner">{error}</div>}

        {!scanned && (
          <div className="action-stack">
            <button className="btn btn-primary" type="button" disabled={busy} onClick={handleScanCommonLocations}>
              {t("onboarding_scan")}
            </button>
            <button className="btn" type="button" disabled={busy} onClick={handleAddFolder}>
              {t("common_add_folder")}
            </button>
            <button className="btn" type="button" disabled={busy} onClick={handleImport}>
              {t("onboarding_import")}
            </button>
            <button className="btn" type="button" disabled={busy} onClick={() => onDone("discover")}>
              {t("common_discover_models")}
            </button>
            <button className="btn btn-ghost" type="button" onClick={() => onDone()} disabled={busy}>
              {t("onboarding_skip")}
            </button>
          </div>
        )}

        {scanned && (
          <div>
            {candidates.length === 0 ? (
              <div className="empty-state" style={{ marginBottom: 16 }}>
                {t("onboarding_empty_scan")}
              </div>
            ) : (
              <div className="panel" style={{ marginBottom: 16 }}>
                <div className="panel-title">{t("onboarding_found_models")}</div>
                {candidates.map((c) => (
                  <div key={c.path} className="kv-row">
                    <span className="kv-row-label mono" style={{ wordBreak: "break-all" }}>
                      {c.path.split(/[\\/]/).pop()}
                    </span>
                  </div>
                ))}
              </div>
            )}

            <div className="action-stack">
              {candidates.length > 0 && (
                <button className="btn btn-primary" type="button" disabled={busy} onClick={handleImportAllDiscovered}>
                  {t("onboarding_import_discovered")}
                </button>
              )}
              <button className="btn" type="button" disabled={busy} onClick={handleAddFolder}>
                {t("common_add_folder")}
              </button>
              <button className="btn" type="button" disabled={busy} onClick={() => onDone("discover")}>
                {t("common_discover_models")}
              </button>
              <button className="btn btn-ghost" type="button" onClick={() => onDone()} disabled={busy}>
                {t("onboarding_skip")}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
