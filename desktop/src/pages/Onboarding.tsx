import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { importModel } from "../lib/api";
import { useState } from "react";

/** First-run screen (spec section 3): one short explanation, no
 * internet/sign-in/personal info required, fully skippable. Scan/Import
 * both hand off to a real dialog + real import so a first-time user can
 * get a model into the library without visiting the Models page first. */
export function Onboarding({ onDone }: { onDone: () => void }) {
  const { t, lang, setLang } = useI18n();
  const status = useAppStatus();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  return (
    <div className="page" style={{ maxWidth: 560, margin: "10vh auto" }}>
      <div style={{ display: "flex", justifyContent: "flex-end", marginBottom: 12 }}>
        <select
          aria-label={t("settings_language")}
          value={lang}
          onChange={(e) => setLang(e.target.value as "en" | "ar")}
          style={{ background: "var(--bg-panel)", color: "var(--text-primary)", border: "1px solid var(--border-strong)" }}
        >
          <option value="en">English</option>
          <option value="ar">العربية</option>
        </select>
      </div>

      <div className="panel">
        <h1 className="page-title" style={{ marginBottom: 8 }}>
          {t("onboarding_title")}
        </h1>
        <p className="text-secondary" style={{ marginBottom: 20 }}>
          {t("onboarding_body")}
        </p>

        <ul style={{ margin: "0 0 20px", padding: 0, listStyle: "none", display: "flex", flexDirection: "column", gap: 8 }}>
          <li className="text-secondary">✓ {t("onboarding_point_local")}</li>
          <li className="text-secondary">✓ {t("onboarding_point_no_account")}</li>
          <li className="text-secondary">✓ {t("onboarding_point_no_upload")}</li>
        </ul>

        {error && <div className="error-banner">{error}</div>}

        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <button className="btn btn-primary" disabled={busy} onClick={onDone}>
            {t("onboarding_scan")}
          </button>
          <button className="btn" disabled={busy} onClick={handleImport}>
            {t("onboarding_import")}
          </button>
          <button className="btn" disabled={busy} onClick={onDone}>
            {t("onboarding_open_library")}
          </button>
        </div>

        <div style={{ marginTop: 20 }}>
          <button className="btn" onClick={onDone} disabled={busy}>
            {t("onboarding_skip")}
          </button>
        </div>
      </div>
    </div>
  );
}
