import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../i18n/I18nContext";
import { listLibrary, recommendModel, tuneCancel, tuneDryRun, tuneRun } from "../lib/api";
import type { LibraryEntry, Priority, RankingPriority, Recommendation, TunePlanDto, TuneRunDto, TuneProgressEvent } from "../lib/types";
import { formatBytes, formatTokensPerSecond } from "../lib/format";
import { useAppStatus } from "../lib/AppStatusContext";

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

  return (
    <div>
      <div className="field" style={{ maxWidth: 320 }}>
        <label>{t("optimize_priority")}</label>
        <select value={priority} onChange={(e) => setPriority(e.target.value as Priority)}>
          {PRIORITIES.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
      </div>
      <button className="btn btn-primary" onClick={run} disabled={busy}>
        {busy ? t("common_loading") : "Recommend"}
      </button>

      {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}

      {result && (
        <div className="panel" style={{ marginTop: 16 }}>
          <h3>{result.recommended.build.display_name}</h3>
          <p className="text-secondary">
            Fit: <span className={`badge badge-${result.recommended.fit.state === "excellent" || result.recommended.fit.state === "good" ? "good" : result.recommended.fit.state === "not_recommended" ? "bad" : "warn"}`}>{result.recommended.fit.state}</span>
          </p>
          <p className="text-secondary">
            Estimated RAM: {formatBytes(result.recommended.estimate.estimated_total_ram_bytes_low)} –{" "}
            {formatBytes(result.recommended.estimate.estimated_total_ram_bytes_high)}
          </p>

          <div style={{ display: "flex", gap: 8, marginTop: 8, marginBottom: 8 }}>
            <button className={`btn ${tab === "simple" ? "btn-primary" : ""}`} onClick={() => setTab("simple")}>
              {t("optimize_simple_tab")}
            </button>
            <button className={`btn ${tab === "technical" ? "btn-primary" : ""}`} onClick={() => setTab("technical")}>
              {t("optimize_technical_tab")}
            </button>
          </div>

          {tab === "simple" ? (
            <p>{result.explanation.simple}</p>
          ) : (
            <dl style={{ margin: 0 }}>
              {Object.entries(result.explanation.technical).map(([k, v]) => (
                <div key={k} style={{ padding: "4px 0", borderBottom: "1px solid var(--border-subtle)" }}>
                  <div className="text-secondary" style={{ fontSize: 11, textTransform: "uppercase" }}>
                    {k}
                  </div>
                  <div>{v ?? "—"}</div>
                </div>
              ))}
            </dl>
          )}

          {result.safer_fallback && (
            <p className="text-tertiary" style={{ marginTop: 8 }}>
              Safer fallback: {result.safer_fallback.build.display_name}
            </p>
          )}
          {result.stronger_optional && (
            <p className="text-tertiary">Stronger optional: {result.stronger_optional.build.display_name}</p>
          )}
        </div>
      )}
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
    return <p className="text-secondary">Set a runtime binary path in Settings before tuning.</p>;
  }

  return (
    <div>
      <div className="field" style={{ maxWidth: 420 }}>
        <label>Model</label>
        <select value={modelId} onChange={(e) => setModelId(e.target.value)}>
          <option value="">Select a model…</option>
          {models.map((m) => (
            <option key={m.library_id} value={m.library_id}>
              {m.alias ?? m.current_path.split(/[\\/]/).pop()}
            </option>
          ))}
        </select>
      </div>

      <div className="field" style={{ maxWidth: 320 }}>
        <label>{t("optimize_priority")}</label>
        <select value={priority} onChange={(e) => setPriority(e.target.value as RankingPriority)}>
          {RANKING_PRIORITIES.map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
        </select>
      </div>

      <label style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 12 }}>
        <input type="checkbox" checked={saveProfile} onChange={(e) => setSaveProfile(e.target.checked)} />
        Save winning profile
      </label>

      <div className="panel" style={{ marginBottom: 12, borderColor: "var(--state-warn)" }}>
        <p>{t("tune_warning_cpu")}</p>
        <p className="text-tertiary">{t("tune_warning_no_system_changes")}</p>
      </div>

      <div style={{ display: "flex", gap: 8 }}>
        <button className="btn" onClick={handleDryRun} disabled={!model || busy || running}>
          {t("tune_dry_run")}
        </button>
        <button className="btn btn-primary" onClick={handleStart} disabled={!model || busy || running}>
          {t("tune_start")}
        </button>
        {running && (
          <button className="btn btn-danger" onClick={handleCancel}>
            {t("tune_cancel")}
          </button>
        )}
      </div>

      {error && <div className="error-banner" style={{ marginTop: 12 }}>{error}</div>}

      {plan && !running && !result && (
        <div className="panel" style={{ marginTop: 16 }}>
          <div className="panel-title">Plan preview</div>
          <p className="text-secondary">{plan.plan.candidates.length} candidate(s), formula {plan.plan.formula_version}</p>
          {plan.backend_verifications.map((v) => (
            <p key={v.backend} className="text-tertiary">
              {v.backend}: {v.status} {v.failure_reason ? `— ${v.failure_reason}` : ""}
            </p>
          ))}
        </div>
      )}

      {running && (
        <div className="panel" style={{ marginTop: 16 }}>
          <div className="panel-title">Progress</div>
          <p>
            {progress ? `${progress.completed_candidates} / ${progress.total_candidates}` : "Starting…"}
            {progress?.current_candidate_id ? ` — ${progress.current_candidate_id}` : ""}
          </p>
        </div>
      )}

      {result && (
        <div className="panel" style={{ marginTop: 16 }}>
          <div className="panel-title">Result</div>
          <p className="text-secondary">
            {result.candidate_results.length} candidate(s) measured in {result.wall_time_secs.toFixed(1)}s
            {result.summary_cancelled ? " (cancelled)" : ""}
          </p>
          {result.ranking.winner ? (
            <>
              <h4 style={{ marginTop: 8 }}>Winner: {result.ranking.winner.candidate_id}</h4>
              <p className="text-secondary">
                {formatTokensPerSecond(result.ranking.winner.measurements.mean_generation_tokens_per_second)} generation,{" "}
                {formatTokensPerSecond(result.ranking.winner.measurements.mean_prompt_tokens_per_second)} prompt · confidence:{" "}
                {result.ranking.confidence}
              </p>
              {result.saved_profile_id && <p className="text-secondary">Saved profile: {result.saved_profile_id}</p>}
            </>
          ) : (
            <p className="text-secondary">No candidate completed successfully.</p>
          )}
          {result.ranking.unknown_values.map((u) => (
            <p key={u} className="text-tertiary">
              {u}
            </p>
          ))}
        </div>
      )}
    </div>
  );
}

export function Optimize() {
  const { t } = useI18n();
  const [tab, setTab] = useState<"recommend" | "tune">("recommend");

  return (
    <div className="page">
      <div className="page-header">
        <h1 className="page-title">{t("optimize_title")}</h1>
      </div>
      <div style={{ display: "flex", gap: 8, marginBottom: 16 }}>
        <button className={`btn ${tab === "recommend" ? "btn-primary" : ""}`} onClick={() => setTab("recommend")}>
          Recommend
        </button>
        <button className={`btn ${tab === "tune" ? "btn-primary" : ""}`} onClick={() => setTab("tune")}>
          {t("tune_title")}
        </button>
      </div>
      {tab === "recommend" ? <RecommendTab /> : <TuneTab />}
    </div>
  );
}
