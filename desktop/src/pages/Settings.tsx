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
        <div>
          <div className="page-kicker">{t("settings_kicker")}</div>
          <h1 className="page-title">{t("settings_title")}</h1>
          <p className="page-desc">{t("settings_desc")}</p>
        </div>
      </div>

      <div className="stack-2">
        <div className="panel">
          <div className="panel-title">{t("settings_display")}</div>
          <div className="field">
            <label htmlFor="settings-lang">{t("settings_language")}</label>
            <select id="settings-lang" value={lang} onChange={(e) => setLang(e.target.value as "en" | "ar")}>
              <option value="en">English</option>
              <option value="ar">العربية</option>
            </select>
          </div>
        </div>

        <div className="panel">
          <div className="panel-title">{t("settings_runtime")}</div>
          <div className="field">
            <label htmlFor="settings-llama">{t("settings_runtime_binary")}</label>
            <div style={{ display: "flex", gap: 8 }}>
              <input id="settings-llama" type="text" readOnly value={status.llamaBinPath || t("status_none")} className="path-text" style={{ flex: 1 }} />
              <button className="btn" type="button" onClick={chooseLlamaBin}>
                {t("settings_choose")}
              </button>
            </div>
          </div>
          <div className="field">
            <label htmlFor="settings-lib">{t("settings_library_location")}</label>
            <input id="settings-lib" type="text" readOnly value={status.libraryPath} className="path-text" />
          </div>
        </div>

        <div className="panel">
          <div className="panel-title">{t("settings_local_state")}</div>
          <p className="text-secondary" style={{ marginBottom: 12 }}>
            {t("settings_privacy_body")}
          </p>
          <p className="path-text text-tertiary" style={{ marginBottom: 12 }}>
            %LOCALAPPDATA%\BruteRuntime\
          </p>
          <button
            className="btn btn-danger"
            type="button"
            onClick={() => {
              if (window.confirm(t("settings_reset_confirm"))) {
                window.localStorage.removeItem(ONBOARDED_KEY);
                window.location.reload();
              }
            }}
          >
            {t("settings_reset_state")}
          </button>
        </div>

        <div className="panel">
          <div className="panel-title">{t("settings_about")}</div>
          <div className="kv-row">
            <span className="kv-row-label">{t("settings_version")}</span>
            <span className="kv-row-value mono">0.1.0</span>
          </div>
          <div className="kv-row">
            <span className="kv-row-label">{t("settings_network")}</span>
            <span className="kv-row-value">{t("status_no_network")}</span>
          </div>
          <p className="text-tertiary" style={{ marginTop: 12 }}>
            {t("dev_build_notice")}
          </p>
        </div>
      </div>
    </div>
  );
}
