import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";

const ONBOARDED_KEY = "brute.onboarded";

export function Settings() {
  const { t, lang, setLang } = useI18n();
  const status = useAppStatus();

  async function chooseLlamaBin() {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") status.setLlamaBinPath(dir);
  }

  return (
    <div className="page">
      <div className="page-header">
        <h1 className="page-title">{t("settings_title")}</h1>
      </div>

      <div className="panel" style={{ marginBottom: 16, maxWidth: 520 }}>
        <div className="field">
          <label>{t("settings_language")}</label>
          <select value={lang} onChange={(e) => setLang(e.target.value as "en" | "ar")}>
            <option value="en">English</option>
            <option value="ar">العربية</option>
          </select>
        </div>

        <div className="field">
          <label>{t("settings_runtime_binary")}</label>
          <div style={{ display: "flex", gap: 8 }}>
            <input type="text" readOnly value={status.llamaBinPath || "Not set"} style={{ flex: 1 }} />
            <button className="btn" onClick={chooseLlamaBin}>
              Choose…
            </button>
          </div>
        </div>

        <div className="field">
          <label>{t("settings_library_location")}</label>
          <input type="text" readOnly value={status.libraryPath} />
        </div>
      </div>

      <div className="panel" style={{ maxWidth: 520 }}>
        <div className="panel-title">Local state</div>
        <p className="text-secondary" style={{ marginBottom: 12 }}>
          Nothing in this application is ever uploaded, synced, or shared automatically. All state — the model
          library, calibration records, and runtime profiles — lives only on this device under
          <span className="mono"> %LOCALAPPDATA%\BruteRuntime\</span>.
        </p>
        <button
          className="btn btn-danger"
          onClick={() => {
            if (window.confirm("Reset onboarding and local UI preferences? This does not delete your model library or profiles.")) {
              window.localStorage.removeItem(ONBOARDED_KEY);
              window.location.reload();
            }
          }}
        >
          {t("settings_reset_state")}
        </button>
      </div>

      <p className="text-tertiary" style={{ marginTop: 24 }}>
        {t("dev_build_notice")}
      </p>
    </div>
  );
}
