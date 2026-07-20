import { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { getAssociations, listLibrary } from "../lib/api";
import type { LibraryEntry, RuntimeProfile } from "../lib/types";
import { formatBytes, formatDate, formatParamCount, formatTokensPerSecond, shortHash } from "../lib/format";
import { TechnicalValue } from "../components/TechnicalValue";
import type { Page } from "../App";

/** A read-only diagnostics view of whichever model/profile Chat or Run
 * currently has active (via AppStatusContext) - not a duplicate of Run's
 * interactive "type a prompt and execute it" workflow, which stays on its
 * own page. Every value here is data BRUTE already has on disk (imported
 * library metadata, a saved tuning profile, the resolved runtime binary
 * path) - nothing here is fetched, scored, or fabricated for this page. */
export function AdvancedConsole({ onNavigate }: { onNavigate: (page: Page) => void }) {
  const { t } = useI18n();
  const status = useAppStatus();
  const [model, setModel] = useState<LibraryEntry | null>(null);
  const [profile, setProfile] = useState<RuntimeProfile | null>(null);

  useEffect(() => {
    if (!status.libraryId) {
      setModel(null);
      return;
    }
    listLibrary()
      .then((list) => setModel(list.find((m) => m.library_id === status.libraryId) ?? null))
      .catch(() => setModel(null));
  }, [status.libraryId]);

  useEffect(() => {
    if (!status.libraryId || !status.profileId) {
      setProfile(null);
      return;
    }
    getAssociations(status.libraryId)
      .then((a) => setProfile(a.runtime_profiles.find((p) => p.profile_id === status.profileId) ?? null))
      .catch(() => setProfile(null));
  }, [status.libraryId, status.profileId]);

  const runtimeRows: [string, string][] = [
    [t("console_source"), status.runtimeResolution?.source ?? "—"],
    [t("run_binary"), status.llamaBinPath || "—"],
    [t("console_cli_verified"), status.runtimeResolution?.cli_verified ? t("common_yes") : t("common_no")],
    [t("console_bench_verified"), status.runtimeResolution?.bench_verified ? t("common_yes") : t("common_no")],
  ];

  const modelRows: [string, string][] = model
    ? [
        [t("models_col_arch"), model.architecture ?? "—"],
        [t("models_col_quant"), model.quantization ?? "—"],
        [t("models_params"), formatParamCount(model.parameter_count)],
        [t("models_col_size"), formatBytes(model.file_size_bytes)],
        [t("models_filter_trust"), model.trust],
        [t("models_col_status"), model.file_status],
        [t("models_imported_at"), formatDate(model.imported_at_rfc3339)],
        ["SHA-256", shortHash(model.sha256)],
      ]
    : [];

  const profileRows: [string, string][] = profile
    ? [
        [t("status_backend"), profile.backend],
        [t("run_threads"), String(profile.threads)],
        [t("run_gpu_layers"), String(profile.gpu_layers)],
        [t("run_context"), String(profile.context_size)],
        [t("run_batch"), String(profile.batch_size)],
        [t("run_measured_gen"), formatTokensPerSecond(profile.mean_generation_tokens_per_second)],
        [t("run_prompt_tps"), formatTokensPerSecond(profile.mean_prompt_tokens_per_second)],
        [t("run_memory_estimate"), formatBytes(profile.predicted_ram_bytes)],
        [t("profiles_stability"), profile.stability],
        [t("profiles_confidence"), profile.confidence],
        [t("profiles_tuned"), formatDate(profile.tuning_date)],
      ]
    : [];

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="page-kicker">{t("console_kicker")}</div>
          <h1 className="page-title">{t("nav_advanced_console")}</h1>
          <p className="page-desc">{t("console_desc")}</p>
        </div>
      </div>

      <div className="panel">
        <div className="panel-title">{t("console_runtime_section")}</div>
        {runtimeRows.map(([label, value]) => (
          <div key={label} className="kv-row">
            <span className="kv-row-label">{label}</span>
            <TechnicalValue className="kv-row-value mono" style={{ wordBreak: "break-all" }}>
              {value}
            </TechnicalValue>
          </div>
        ))}
      </div>

      <div className="panel">
        <div className="panel-title">{t("status_active_model")}</div>
        {!model ? (
          <p className="text-tertiary">{t("console_no_model")}</p>
        ) : (
          <>
            <TechnicalValue as="h3" className="inspector-title">
              {model.alias ?? model.current_path.split(/[\\/]/).pop()}
            </TechnicalValue>
            {modelRows.map(([label, value]) => (
              <div key={label} className="kv-row">
                <span className="kv-row-label">{label}</span>
                <TechnicalValue className="kv-row-value mono">{value}</TechnicalValue>
              </div>
            ))}
          </>
        )}
      </div>

      <div className="panel">
        <div className="panel-title">{t("status_active_profile")}</div>
        {!profile ? (
          <p className="text-tertiary">{t("console_no_profile")}</p>
        ) : (
          <>
            <TechnicalValue as="h3" className="inspector-title">
              {profile.profile_id}
            </TechnicalValue>
            {profileRows.map(([label, value]) => (
              <div key={label} className="kv-row">
                <span className="kv-row-label">{label}</span>
                <TechnicalValue className="kv-row-value mono">{value}</TechnicalValue>
              </div>
            ))}
          </>
        )}
      </div>

      <div className="panel">
        <button className="btn btn-primary" type="button" onClick={() => onNavigate("run")}>
          {t("console_open_run")}
        </button>
        <p className="text-tertiary" style={{ marginTop: 8, fontSize: 11 }}>
          {t("console_logs_note")}
        </p>
      </div>
    </div>
  );
}
