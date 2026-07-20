import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { brand } from "../config/brand";
import { TechnicalValue } from "../components/TechnicalValue";

const ONBOARDED_KEY = "brute.onboarded";

function runtimeSourceTone(source: string | undefined): "good" | "warn" | "bad" | "unknown" {
  switch (source) {
    case "bundled":
      return "good";
    case "user_override":
    case "discovered":
      return "warn";
    case "not_found":
      return "bad";
    default:
      return "unknown";
  }
}

export function Settings() {
  const { t, lang, setLang } = useI18n();
  const status = useAppStatus();

  async function chooseRuntimeOverride() {
    const dir = await open({ directory: true, multiple: false });
    if (typeof dir === "string") {
      status.setManualRuntimeOverride(dir);
    }
  }

  async function clearRuntimeOverride() {
    status.setManualRuntimeOverride("");
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
          <div className="kv-row">
            <span className="kv-row-label">{t("settings_runtime_status")}</span>
            <span className="kv-row-value">
              <span className={`badge badge-${runtimeSourceTone(status.runtimeResolution?.source)}`}>
                {status.runtimeResolution ? t(`settings_runtime_source_${status.runtimeResolution.source}`) : t("common_loading")}
              </span>
            </span>
          </div>
          {status.runtimeResolution?.binary_dir && (
            <div className="kv-row">
              <span className="kv-row-label">{t("settings_runtime_binary")}</span>
              <TechnicalValue className="kv-row-value mono path-text">{status.runtimeResolution.binary_dir}</TechnicalValue>
            </div>
          )}
          <div className="kv-row">
            <span className="kv-row-label">{t("settings_runtime_verification")}</span>
            <span className="kv-row-value">
              {status.runtimeResolution ? (
                <>
                  <TechnicalValue as="span" className={`badge ${status.runtimeResolution.cli_verified ? "badge-good" : "badge-unknown"}`}>
                    llama-cli
                  </TechnicalValue>
                  <TechnicalValue
                    as="span"
                    className={`badge ${status.runtimeResolution.bench_verified ? "badge-good" : "badge-unknown"}`}
                    style={{ marginInlineStart: 6 }}
                  >
                    llama-bench
                  </TechnicalValue>
                </>
              ) : (
                "—"
              )}
            </span>
          </div>
          <p className="text-secondary" style={{ marginTop: 8, fontSize: 11.5 }}>
            {status.runtimeResolution?.detail}
          </p>
          <button className="btn" type="button" style={{ marginTop: 8 }} onClick={() => status.refreshRuntimeResolution()}>
            {t("settings_runtime_recheck")}
          </button>

          <div className="field" style={{ marginTop: 8 }}>
            <label htmlFor="settings-lib">{t("settings_library_location")}</label>
            <input id="settings-lib" type="text" dir="ltr" readOnly value={status.libraryPath} className="path-text" />
          </div>
        </div>

        <div className="panel">
          <div className="panel-title">{t("settings_advanced")}</div>
          <p className="text-secondary" style={{ marginBottom: 12, fontSize: 11.5 }}>
            {t("settings_advanced_override_hint")}
          </p>
          <div className="field">
            <label htmlFor="settings-runtime-override">{t("settings_advanced_override")}</label>
            <div style={{ display: "flex", gap: 8 }}>
              <input
                id="settings-runtime-override"
                type="text"
                dir="ltr"
                readOnly
                value={status.manualRuntimeOverride || t("status_none")}
                className="path-text"
                style={{ flex: 1 }}
              />
              <button className="btn" type="button" onClick={chooseRuntimeOverride}>
                {t("settings_choose")}
              </button>
              {status.manualRuntimeOverride && (
                <button className="btn btn-ghost" type="button" onClick={clearRuntimeOverride}>
                  {t("common_clear")}
                </button>
              )}
            </div>
          </div>
        </div>

        <div className="panel">
          <div className="panel-title">{t("settings_local_state")}</div>
          <p className="text-secondary" style={{ marginBottom: 12 }}>
            {t("settings_privacy_body")}
          </p>
          <TechnicalValue as="p" className="path-text text-tertiary" style={{ marginBottom: 12 }}>
            %LOCALAPPDATA%\BruteRuntime\
          </TechnicalValue>
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

          <img className="about-logo" src={brand.logoUrl} alt={`${brand.productName} — ${brand.tagline}`} />
          <div className="about-product-name">{brand.productName}</div>
          <div className="about-slogan">{t("slogan")}</div>

          <div className="about-desc">
            <p>{t("settings_about_desc_1")}</p>
            <p>{t("settings_about_desc_2")}</p>
          </div>

          <div className="kv-row">
            <span className="kv-row-label">{t("settings_version")}</span>
            <TechnicalValue className="kv-row-value mono">0.1.0</TechnicalValue>
          </div>
          <div className="kv-row">
            <span className="kv-row-label">{t("settings_network")}</span>
            <span className="kv-row-value">{t("status_no_network")}</span>
          </div>
          <div className="kv-row">
            <span className="kv-row-label">{t("overview_privacy")}</span>
            <span className="kv-row-value">{t("overview_privacy_value")}</span>
          </div>
          <div className="kv-row">
            <span className="kv-row-label">{t("settings_signing")}</span>
            <span className="kv-row-value">{t("dev_build_notice")}</span>
          </div>

          <div className="about-credit">{t("settings_author_credit")}</div>
        </div>
      </div>
    </div>
  );
}
