import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { brand } from "../config/brand";
import { TechnicalValue } from "../components/TechnicalValue";
import { getPreferences, resetPreferences, savePreferences } from "../lib/api";
import type { GpuPreference, LanguagePreference, Preferences, SpeedQualityPriority, UseCase } from "../lib/types";

const ONBOARDED_KEY = "brute.onboarded";

const GB = 1_000_000_000;

function bytesToGbInput(bytes: number | null): string {
  return bytes === null ? "" : String(bytes / GB);
}

function gbInputToBytes(text: string): number | null {
  const n = Number(text);
  return text.trim() === "" || Number.isNaN(n) || n <= 0 ? null : Math.round(n * GB);
}

function parseList(text: string): string[] {
  return text
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);
}

function PreferencesPanel() {
  const { t } = useI18n();
  const [prefs, setPrefs] = useState<Preferences | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getPreferences()
      .then(setPrefs)
      .catch(() => setError(t("pref_load_error")));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function update(patch: Partial<Preferences>) {
    if (!prefs) return;
    const next = { ...prefs, ...patch };
    setPrefs(next);
    setError(null);
    try {
      await savePreferences(next);
    } catch (e) {
      setError(String(e));
    }
  }

  async function handleReset() {
    if (!window.confirm(t("pref_reset_confirm"))) return;
    try {
      const defaults = await resetPreferences();
      setPrefs(defaults);
      setError(null);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="panel">
      <div className="panel-title">{t("settings_preferences_title")}</div>
      <p className="text-secondary" style={{ marginBottom: 12, fontSize: 11.5 }}>
        {t("settings_preferences_desc")}
      </p>
      {error && (
        <div className="error-banner" style={{ marginBottom: 12 }}>
          {error}
        </div>
      )}
      {!prefs ? (
        <p className="text-tertiary">{t("common_loading")}</p>
      ) : (
        <>
          <div className="field">
            <label htmlFor="pref-language">{t("pref_language")}</label>
            <select
              id="pref-language"
              value={prefs.language}
              onChange={(e) => update({ language: e.target.value as LanguagePreference })}
            >
              <option value="arabic">{t("pref_language_arabic")}</option>
              <option value="english">{t("pref_language_english")}</option>
              <option value="both">{t("pref_language_both")}</option>
            </select>
          </div>

          <div className="field">
            <label htmlFor="pref-use">{t("pref_use")}</label>
            <select id="pref-use" value={prefs.use_case} onChange={(e) => update({ use_case: e.target.value as UseCase })}>
              <option value="general_assistant">{t("pref_use_general_assistant")}</option>
              <option value="coding">{t("pref_use_coding")}</option>
              <option value="documents">{t("pref_use_documents")}</option>
              <option value="writing">{t("pref_use_writing")}</option>
              <option value="summarization">{t("pref_use_summarization")}</option>
              <option value="reasoning">{t("pref_use_reasoning")}</option>
            </select>
          </div>

          <div className="field">
            <label htmlFor="pref-priority">{t("pref_priority")}</label>
            <select
              id="pref-priority"
              value={prefs.priority}
              onChange={(e) => update({ priority: e.target.value as SpeedQualityPriority })}
            >
              <option value="fastest">{t("pref_priority_fastest")}</option>
              <option value="balanced">{t("pref_priority_balanced")}</option>
              <option value="best_quality">{t("pref_priority_best_quality")}</option>
            </select>
          </div>

          <button
            className="btn btn-ghost btn-sm"
            type="button"
            onClick={() => setAdvancedOpen((v) => !v)}
            style={{ marginTop: 8 }}
          >
            {advancedOpen ? t("pref_advanced_hide") : t("pref_advanced_show")}
          </button>

          {advancedOpen && (
            <div style={{ marginTop: 12 }}>
              <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={prefs.arabic_priority}
                  onChange={(e) => update({ arabic_priority: e.target.checked })}
                />
                {t("pref_arabic_priority")}
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={prefs.english_priority}
                  onChange={(e) => update({ english_priority: e.target.checked })}
                />
                {t("pref_english_priority")}
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={prefs.memory_conservative_mode}
                  onChange={(e) => update({ memory_conservative_mode: e.target.checked })}
                />
                {t("pref_memory_conservative")}
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
                <input type="checkbox" checked={prefs.cpu_only} onChange={(e) => update({ cpu_only: e.target.checked })} />
                {t("pref_cpu_only")}
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={prefs.offline_only}
                  onChange={(e) => update({ offline_only: e.target.checked })}
                />
                {t("pref_offline_only")}
              </label>
              <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 10 }}>
                <input
                  type="checkbox"
                  checked={prefs.commercial_use_required}
                  onChange={(e) => update({ commercial_use_required: e.target.checked })}
                />
                {t("pref_commercial_use_required")}
              </label>

              <div className="field">
                <label htmlFor="pref-gpu">{t("pref_gpu_preference")}</label>
                <select
                  id="pref-gpu"
                  value={prefs.gpu_preference}
                  onChange={(e) => update({ gpu_preference: e.target.value as GpuPreference })}
                >
                  <option value="no_preference">{t("pref_gpu_no_preference")}</option>
                  <option value="prefer_gpu">{t("pref_gpu_prefer")}</option>
                  <option value="require_gpu">{t("pref_gpu_require")}</option>
                </select>
              </div>

              <div className="field">
                <label htmlFor="pref-licenses">{t("pref_permitted_licenses")}</label>
                <input
                  id="pref-licenses"
                  type="text"
                  dir="ltr"
                  value={prefs.permitted_licenses.join(", ")}
                  onChange={(e) => update({ permitted_licenses: parseList(e.target.value) })}
                />
              </div>

              <div className="field">
                <label htmlFor="pref-preferred-families">{t("pref_preferred_families")}</label>
                <input
                  id="pref-preferred-families"
                  type="text"
                  dir="ltr"
                  value={prefs.preferred_families.join(", ")}
                  onChange={(e) => update({ preferred_families: parseList(e.target.value) })}
                />
              </div>

              <div className="field">
                <label htmlFor="pref-excluded-families">{t("pref_excluded_families")}</label>
                <input
                  id="pref-excluded-families"
                  type="text"
                  dir="ltr"
                  value={prefs.excluded_families.join(", ")}
                  onChange={(e) => update({ excluded_families: parseList(e.target.value) })}
                />
              </div>

              <div className="field">
                <label htmlFor="pref-max-download">{t("pref_max_download_size")}</label>
                <input
                  id="pref-max-download"
                  type="number"
                  min="0"
                  dir="ltr"
                  value={bytesToGbInput(prefs.max_download_size_bytes)}
                  onChange={(e) => update({ max_download_size_bytes: gbInputToBytes(e.target.value) })}
                />
              </div>
              <div className="field">
                <label htmlFor="pref-max-ram">{t("pref_max_ram")}</label>
                <input
                  id="pref-max-ram"
                  type="number"
                  min="0"
                  dir="ltr"
                  value={bytesToGbInput(prefs.max_ram_bytes)}
                  onChange={(e) => update({ max_ram_bytes: gbInputToBytes(e.target.value) })}
                />
              </div>
              <div className="field">
                <label htmlFor="pref-max-vram">{t("pref_max_vram")}</label>
                <input
                  id="pref-max-vram"
                  type="number"
                  min="0"
                  dir="ltr"
                  value={bytesToGbInput(prefs.max_vram_bytes)}
                  onChange={(e) => update({ max_vram_bytes: gbInputToBytes(e.target.value) })}
                />
              </div>

              <button className="btn btn-ghost" type="button" onClick={handleReset} style={{ marginTop: 8 }}>
                {t("pref_reset")}
              </button>
            </div>
          )}
        </>
      )}
    </div>
  );
}

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

        <PreferencesPanel />

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
