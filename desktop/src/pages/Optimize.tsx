import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../i18n/I18nContext";
import { listLibrary, recommendModel, tuneCancel, tuneDryRun, tuneRun } from "../lib/api";
import type { LibraryEntry, Priority, RankingPriority, Recommendation, TunePlanDto, TuneRunDto, TuneProgressEvent } from "../lib/types";
import { formatBytes, formatTokensPerSecond } from "../lib/format";
import { useAppStatus } from "../lib/AppStatusContext";
import { TechnicalValue } from "../components/TechnicalValue";

const PRIORITIES: Priority[] = [
  "fastest",
  "balanced",
  "highest_quality",
  "lowest_memory",
  "longest_context",
  "coding",
  "arabic_general_chat",
  "privacy_offline",
];

const RANKING_PRIORITIES: RankingPriority[] = [
  "balanced",
  "fastest_generation",
  "fastest_prompt_processing",
  "lowest_memory",
  "longest_context",
  "maximum_stability",
  "laptop_friendly",
];

function RecommendTab() {
  const { t } = useI18n();
  const [priority, setPriority] = useState<Priority>("balanced");
  const [result, setResult] = useState<Recommendation | null>(null);
  const [tab, setTab] = useState<"simple" | "technical">("simple");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function run() {
    setBusy(true);
    setError(null);
    try {
      const dto = await recommendModel(null, priority);
      setResult(dto.recommendation);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const fitTone =
    result?.recommended.fit.state === "excellent" || result?.recommended.fit.state === "good"
      ? "good"
      : result?.recommended.fit.state === "not_recommended"
        ? "bad"
        : "warn";

  return (
    <div className="stack-2">
      <div className="panel">
        <div className="panel-title">{t("optimize_recommend_title")}</div>
        <div className="field">
          <label htmlFor="rec-priority">{t("optimize_priority")}</label>
          <select id="rec-priority" value={priority} onChange={(e) => setPriority(e.target.value as Priority)}>
            {PRIORITIES.map((p) => (
              <option key={p} value={p} dir="ltr">
                {p}
              </option>
            ))}
          </select>
        </div>
        <button className="btn btn-primary" type="button" onClick={run} disabled={busy}>
          {busy ? t("common_loading") : t("optimize_recommend_action")}
        </button>
        {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}
      </div>

      <div className="panel">
        <div className="panel-title">{t("optimize_result")}</div>
        {!result && <p className="text-tertiary">{t("optimize_result_empty")}</p>}
        {result && (
          <>
            <TechnicalValue as="h3" style={{ fontSize: 15, marginBottom: 8 }}>
              {result.recommended.build.display_name}
            </TechnicalValue>
            <div className="chip-row" style={{ marginBottom: 10 }}>
              <TechnicalValue as="span" className={`badge badge-${fitTone}`}>
                {result.recommended.fit.state}
              </TechnicalValue>
              <span className="badge badge-inferred">{t("common_inferred")}</span>
            </div>
            <p className="text-secondary" style={{ marginBottom: 10 }}>
              {t("optimize_est_ram")}: {formatBytes(result.recommended.estimate.estimated_total_ram_bytes_low)} –{" "}
              {formatBytes(result.recommended.estimate.estimated_total_ram_bytes_high)}
            </p>
            <div className="tab-row">
              <button type="button" className={`tab-btn ${tab === "simple" ? "is-active" : ""}`} onClick={() => setTab("simple")}>
                {t("optimize_simple_tab")}
              </button>
              <button
                type="button"
                className={`tab-btn ${tab === "technical" ? "is-active" : ""}`}
                onClick={() => setTab("technical")}
              >
                {t("optimize_technical_tab")}
              </button>
            </div>
            {tab === "simple" ? (
              <p>{result.explanation.simple}</p>
            ) : (
              <dl style={{ margin: 0 }}>
                {Object.entries(result.explanation.technical).map(([k, v]) => (
                  <div key={k} className="kv-row">
                    <TechnicalValue as="span" className="kv-row-label">
                      {k}
                    </TechnicalValue>
                    <TechnicalValue as="span" className="kv-row-value">
                      {v ?? "—"}
                    </TechnicalValue>
                  </div>
                ))}
              </dl>
            )}
            {result.safer_fallback && (
              <p className="text-tertiary" style={{ marginTop: 10 }}>
                {t("overview_safer_fallback")}: <TechnicalValue as="span">{result.safer_fallback.build.display_name}</TechnicalValue>
              </p>
            )}
            {result.stronger_optional && (
              <p className="text-tertiary">
                {t("optimize_stronger")}: <TechnicalValue as="span">{result.stronger_optional.build.display_name}</TechnicalValue>
              </p>
            )}
          </>
        )}
      </div>
    </div>
  );
}

function TuneTab() {
  const { t } = useI18n();
  const status = useAppStatus();
  const [models, setModels] = useState<LibraryEntry[]>([]);
  const [modelId, setModelId] = useState<string>("");
  const [priority, setPriority] = useState<RankingPriority>("balanced");
  const [saveProfile, setSaveProfile] = useState(true);
  const [plan, setPlan] = useState<TunePlanDto | null>(null);
  const [progress, setProgress] = useState<TuneProgressEvent | null>(null);
  const [result, setResult] = useState<TuneRunDto | null>(null);
  const [running, setRunning] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listLibrary().then(setModels).catch(() => setModels([]));
  }, []);

  useEffect(() => {
    const unlisten = listen<TuneProgressEvent>("tune-progress", (event) => setProgress(event.payload));
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  const model = models.find((m) => m.library_id === modelId);

  async function handleDryRun() {
    if (!model || !status.llamaBinPath) return;
    setBusy(true);
    setError(null);
    try {
      const dto = await tuneDryRun(model.current_path, status.llamaBinPath, null, false);
      setPlan(dto);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleStart() {
    if (!model || !status.llamaBinPath) return;
    setRunning(true);
    setError(null);
    setResult(null);
    setProgress(null);
    try {
      const dto = await tuneRun({
        model: model.current_path,
        llama_bin: status.llamaBinPath,
        priority,
        backend: null,
        max_duration_secs: null,
        allow_unverified_binary: false,
        save_profile: saveProfile,
      });
      setResult(dto);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  async function handleCancel() {
    await tuneCancel().catch(() => undefined);
  }

  if (!status.llamaBinPath) {
    return <div className="warn-banner">{t("tune_need_binary")}</div>;
  }

  const progressPct =
    progress && progress.total_candidates > 0
      ? Math.round((progress.completed_candidates / progress.total_candidates) * 100)
      : running
        ? 0
        : null;

  return (
    <div className="stack-2">
      <div className="panel">
        <div className="panel-title">{t("tune_procedure")}</div>
        <div className="field">
          <label htmlFor="tune-model">{t("run_model")}</label>
          <select id="tune-model" value={modelId} onChange={(e) => setModelId(e.target.value)}>
            <option value="">{t("run_select_model")}</option>
            {models.map((m) => (
              <option key={m.library_id} value={m.library_id} dir="ltr">
                {m.alias ?? m.current_path.split(/[\\/]/).pop()}
              </option>
            ))}
          </select>
        </div>
        <div className="field">
          <label htmlFor="tune-priority">{t("optimize_priority")}</label>
          <select id="tune-priority" value={priority} onChange={(e) => setPriority(e.target.value as RankingPriority)}>
            {RANKING_PRIORITIES.map((p) => (
              <option key={p} value={p} dir="ltr">
                {p}
              </option>
            ))}
          </select>
        </div>
        <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 12 }}>
          <input type="checkbox" checked={saveProfile} onChange={(e) => setSaveProfile(e.target.checked)} />
          {t("tune_save_profile")}
        </label>
        <div className="warn-banner">
          <p>{t("tune_warning_cpu")}</p>
          <p className="text-tertiary" style={{ marginTop: 6 }}>
            {t("tune_warning_no_system_changes")}
          </p>
        </div>
        <div className="page-actions">
          <button className="btn" type="button" onClick={handleDryRun} disabled={!model || busy || running}>
            {t("tune_dry_run")}
          </button>
          <button className="btn btn-primary" type="button" onClick={handleStart} disabled={!model || busy || running}>
            {t("tune_start")}
          </button>
          {running && (
            <button className="btn btn-danger" type="button" onClick={handleCancel}>
              {t("tune_cancel")}
            </button>
          )}
        </div>
        {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}
      </div>

      <div className="panel">
        <div className="panel-title">{t("tune_status_panel")}</div>
        {plan && !running && !result && (
          <>
            <p className="text-secondary">
              {plan.plan.candidates.length} {t("tune_candidates")} · {plan.plan.formula_version}
            </p>
            {plan.backend_verifications.map((v) => (
              <div key={v.backend} className="kv-row">
                <TechnicalValue as="span" className="kv-row-label">
                  {v.backend}
                </TechnicalValue>
                <span className="kv-row-value">
                  <TechnicalValue
                    as="span"
                    className={`badge badge-${v.status === "verified" ? "good" : v.status === "detected_only" ? "detected" : "unknown"}`}
                  >
                    {v.status}
                  </TechnicalValue>
                </span>
              </div>
            ))}
          </>
        )}

        {running && (
          <>
            <p className="text-secondary" style={{ marginBottom: 8 }}>
              <TechnicalValue as="span">
                {progress ? `${progress.completed_candidates} / ${progress.total_candidates}` : t("common_loading")}
              </TechnicalValue>
              {progress?.current_candidate_id ? (
                <>
                  {" — "}
                  <TechnicalValue as="span">{progress.current_candidate_id}</TechnicalValue>
                </>
              ) : (
                ""
              )}
            </p>
            {progressPct !== null && (
              <div className="meter" role="progressbar" aria-valuenow={progressPct} aria-valuemin={0} aria-valuemax={100}>
                <div className="meter-fill" style={{ width: `${progressPct}%` }} />
              </div>
            )}
            {progress?.current_candidate_id && (
              <TechnicalValue as="p" className="text-tertiary mono" style={{ marginTop: 8, fontSize: 11 }}>
                {progress.current_candidate_id}
              </TechnicalValue>
            )}
          </>
        )}

        {result && (
          <>
            <p className="text-secondary">
              {result.candidate_results.length} {t("tune_candidates")} · {result.wall_time_secs.toFixed(1)}s
              {result.summary_cancelled ? ` (${t("tune_cancelled")})` : ""}
            </p>
            {result.ranking.winner ? (
              <>
                <h4 style={{ marginTop: 10 }}>
                  {t("tune_winner")}: <TechnicalValue as="span">{result.ranking.winner.candidate_id}</TechnicalValue>
                </h4>
                <p className="text-secondary">
                  <TechnicalValue as="span">
                    {formatTokensPerSecond(result.ranking.winner.measurements.mean_generation_tokens_per_second)} ·{" "}
                    {formatTokensPerSecond(result.ranking.winner.measurements.mean_prompt_tokens_per_second)} · {result.ranking.confidence}
                  </TechnicalValue>
                </p>
                <span className="badge badge-measured">{t("common_measured")}</span>
                {result.saved_profile_id && (
                  <p className="text-secondary" style={{ marginTop: 8 }}>
                    {t("tune_saved_profile")}: <TechnicalValue as="span">{result.saved_profile_id}</TechnicalValue>
                  </p>
                )}
              </>
            ) : (
              <p className="text-secondary">{t("tune_no_winner")}</p>
            )}
            {result.ranking.runner_up && (
              <p className="text-tertiary" style={{ marginTop: 8 }}>
                {t("tune_runner_up")}: <TechnicalValue as="span">{result.ranking.runner_up.candidate_id}</TechnicalValue>
              </p>
            )}
            {result.ranking.safer_fallback && (
              <p className="text-tertiary">
                {t("overview_safer_fallback")}: <TechnicalValue as="span">{result.ranking.safer_fallback.candidate_id}</TechnicalValue>
              </p>
            )}
            {result.ranking.unknown_values.map((u) => (
              <p key={u} className="text-tertiary">
                {u}
              </p>
            ))}
          </>
        )}

        {!plan && !running && !result && <p className="text-tertiary">{t("tune_status_empty")}</p>}
      </div>
    </div>
  );
}

export function Optimize() {
  const { t } = useI18n();
  const [tab, setTab] = useState<"recommend" | "tune">("recommend");

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="page-kicker">{t("optimize_kicker")}</div>
          <h1 className="page-title">{t("optimize_title")}</h1>
          <p className="page-desc">{t("optimize_desc")}</p>
        </div>
      </div>
      <div className="tab-row">
        <button type="button" className={`tab-btn ${tab === "recommend" ? "is-active" : ""}`} onClick={() => setTab("recommend")}>
          {t("optimize_recommend_title")}
        </button>
        <button type="button" className={`tab-btn ${tab === "tune" ? "is-active" : ""}`} onClick={() => setTab("tune")}>
          {t("tune_title")}
        </button>
      </div>
      {tab === "recommend" ? <RecommendTab /> : <TuneTab />}
    </div>
  );
}
