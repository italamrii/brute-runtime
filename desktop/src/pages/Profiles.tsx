import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { deleteProfile, exportProfile, listLibrary, listProfiles, showProfile, verifyProfile } from "../lib/api";
import type { ApplyResult, LibraryEntry, RuntimeProfile } from "../lib/types";
import { formatTokensPerSecond } from "../lib/format";
import { useAppStatus } from "../lib/AppStatusContext";

export function Profiles() {
  const { t } = useI18n();
  const status = useAppStatus();
  const [ids, setIds] = useState<string[]>([]);
  const [selected, setSelected] = useState<RuntimeProfile | null>(null);
  const [models, setModels] = useState<LibraryEntry[]>([]);
  const [applyResult, setApplyResult] = useState<ApplyResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    listProfiles()
      .then(setIds)
      .catch((e) => setError(String(e)));
  }

  useEffect(refresh, []);
  useEffect(() => {
    listLibrary().then(setModels).catch(() => setModels([]));
  }, []);

  async function select(id: string) {
    setApplyResult(null);
    setError(null);
    try {
      setSelected(await showProfile(id));
    } catch (e) {
      setError(String(e));
    }
  }

  const modelForProfile = selected ? models.find((m) => m.sha256 === selected.model_sha256) : undefined;

  async function handleVerify() {
    if (!selected || !modelForProfile || !status.llamaBinPath) return;
    setBusy(true);
    setError(null);
    try {
      const result = await verifyProfile(selected.profile_id, modelForProfile.current_path, status.llamaBinPath, false, 60);
      setApplyResult(result);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleExport() {
    if (!selected) return;
    const target = await save({ defaultPath: `${selected.profile_id}.json` });
    if (!target) return;
    setBusy(true);
    try {
      await exportProfile(selected.profile_id, target);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete() {
    if (!selected) return;
    setBusy(true);
    try {
      await deleteProfile(selected.profile_id);
      setSelected(null);
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
          <div className="page-kicker">{t("profiles_kicker")}</div>
          <h1 className="page-title">{t("profiles_title")}</h1>
          <p className="page-desc">{t("profiles_desc")}</p>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {ids.length === 0 && <div className="empty-state">{t("profiles_empty")}</div>}

      {ids.length > 0 && (
        <div className={`workspace-split ${selected ? "" : "is-single"}`}>
          <div className="panel">
            <div className="panel-title">{t("profiles_list")}</div>
            <ul className="profile-list">
              {ids.map((id) => (
                <li key={id}>
                  <button
                    type="button"
                    className="nav-item"
                    aria-current={selected?.profile_id === id ? "page" : undefined}
                    onClick={() => select(id)}
                    style={{ width: "100%" }}
                  >
                    {id}
                  </button>
                </li>
              ))}
            </ul>
          </div>

          {selected && (
            <aside className="inspector" aria-label={t("common_details")}>
              <h3 className="inspector-title">{selected.profile_id}</h3>
              <p className="text-tertiary" style={{ marginBottom: 12 }}>
                {t("profiles_tuned")}: {selected.tuning_date}
              </p>

              {[
                [t("status_backend"), selected.backend],
                [t("run_threads"), String(selected.threads)],
                [t("run_gpu_layers"), String(selected.gpu_layers)],
                [t("run_context"), String(selected.context_size)],
                [t("run_batch"), String(selected.batch_size)],
                [t("run_generation"), formatTokensPerSecond(selected.mean_generation_tokens_per_second)],
                [t("run_prompt_tps"), formatTokensPerSecond(selected.mean_prompt_tokens_per_second)],
                [t("profiles_stability"), selected.stability],
                [t("profiles_confidence"), selected.confidence],
                [t("profiles_reps"), `${selected.source_repetitions_succeeded}/${selected.source_repetitions_requested}`],
              ].map(([label, value]) => (
                <div key={String(label)} className="kv-row">
                  <span className="kv-row-label">{label}</span>
                  <span className="kv-row-value mono num">{value}</span>
                </div>
              ))}

              {!modelForProfile && <p className="text-tertiary" style={{ marginTop: 10 }}>{t("profiles_model_missing")}</p>}

              <div className="action-stack" style={{ marginTop: 14 }}>
                <button className="btn" type="button" onClick={handleVerify} disabled={busy || !modelForProfile || !status.llamaBinPath}>
                  {t("common_verify")}
                </button>
                <button className="btn" type="button" onClick={handleExport} disabled={busy}>
                  {t("common_export")}
                </button>
                <button
                  className="btn btn-primary"
                  type="button"
                  onClick={() => status.setActiveProfile(selected.profile_id, selected.backend)}
                  disabled={busy}
                >
                  {t("profiles_set_active")}
                </button>
                <button className="btn btn-danger" type="button" onClick={handleDelete} disabled={busy}>
                  {t("profiles_remove_meta")}
                </button>
              </div>
              <p className="text-tertiary" style={{ marginTop: 8, fontSize: 11 }}>
                {t("profiles_remove_note")}
              </p>

              {applyResult && (
                <div
                  className="panel"
                  style={{
                    marginTop: 12,
                    borderColor: applyResult.status === "verified" ? "var(--state-good)" : "var(--state-bad)",
                  }}
                >
                  <p>
                    {t("run_status")}: <strong>{applyResult.status}</strong>
                  </p>
                  <p className="text-secondary">{applyResult.detail}</p>
                  {applyResult.rollback_recommended && <p className="text-tertiary">{t("profiles_rollback")}</p>}
                  {applyResult.compatibility_issues.map((issue) => (
                    <p key={issue} className="text-tertiary">
                      {issue}
                    </p>
                  ))}
                </div>
              )}
            </aside>
          )}
        </div>
      )}
    </div>
  );
}
