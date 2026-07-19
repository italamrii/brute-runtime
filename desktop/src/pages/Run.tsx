import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../i18n/I18nContext";
import { getAssociations, listLibrary, localRunCancel, localRunGenerate } from "../lib/api";
import type { LibraryEntry, LocalRunOutcome, RunPhase, RuntimeProfile } from "../lib/types";
import { formatBytes, formatTokensPerSecond } from "../lib/format";
import { useAppStatus } from "../lib/AppStatusContext";

export function Run() {
  const { t } = useI18n();
  const status = useAppStatus();
  const [models, setModels] = useState<LibraryEntry[]>([]);
  const [modelId, setModelId] = useState<string>("");
  const [profiles, setProfiles] = useState<RuntimeProfile[]>([]);
  const [profileId, setProfileId] = useState<string>("");
  const [prompt, setPrompt] = useState("");
  const [output, setOutput] = useState("");
  const [running, setRunning] = useState(false);
  const [outcome, setOutcome] = useState<LocalRunOutcome | null>(null);
  const [phase, setPhase] = useState<RunPhase | null>(null);
  const [error, setError] = useState<string | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listLibrary().then((list) => {
      setModels(list);
      if (status.libraryId && list.some((m) => m.library_id === status.libraryId)) {
        setModelId(status.libraryId);
      }
    }).catch(() => setModels([]));
  }, [status.libraryId]);

  useEffect(() => {
    if (!modelId) {
      setProfiles([]);
      setProfileId("");
      return;
    }
    getAssociations(modelId)
      .then((a) => {
        setProfiles(a.runtime_profiles);
        const preferred =
          (status.profileId && a.runtime_profiles.find((p) => p.profile_id === status.profileId)?.profile_id) ||
          a.runtime_profiles[0]?.profile_id ||
          "";
        setProfileId(preferred);
      })
      .catch(() => setProfiles([]));
  }, [modelId, status.profileId]);

  useEffect(() => {
    const unlisten = listen<string>("local-run-chunk", (event) => {
      setOutput((prev) => prev + event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<RunPhase>("local-run-phase", (event) => {
      setPhase(event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    outputRef.current?.scrollTo({ top: outputRef.current.scrollHeight });
  }, [output]);

  const activeProfile = profiles.find((p) => p.profile_id === profileId) ?? null;

  async function handleRun() {
    if (!modelId || !profileId || !status.llamaBinPath) return;
    setRunning(true);
    setError(null);
    setOutcome(null);
    setPhase("preparing");
    setOutput("");
    const model = models.find((m) => m.library_id === modelId);
    status.setActiveModel(modelId, model?.alias ?? model?.current_path.split(/[\\/]/).pop() ?? null);
    const profile = profiles.find((p) => p.profile_id === profileId);
    status.setActiveProfile(profileId, profile?.backend ?? null);
    try {
      const result = await localRunGenerate({
        library_id: modelId,
        profile_id: profileId,
        llama_bin: status.llamaBinPath,
        prompt,
        allow_unverified_binary: false,
      });
      setOutcome(result);
      if (result.error) setError(result.error);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }

  async function handleStop() {
    await localRunCancel().catch(() => undefined);
  }

  const ready = Boolean(modelId && profileId && status.llamaBinPath);
  // Prefer the real phase signal emitted by the Rust side (spec:
  // "required run states") - it reflects genuine engine events (a real
  // binary check, the first real byte of output, a real cancellation
  // request), not a guess derived only from `running`/`outcome`. Falls
  // back to a coarse idle/outcome-based label before the first run ever
  // starts or if a phase event was somehow missed.
  const runState = phase
    ? t(`run_phase_${phase}`)
    : outcome?.cancelled
      ? t("run_state_stopped")
      : outcome?.timed_out
        ? t("run_state_failed")
        : outcome?.succeeded
          ? t("run_state_complete")
          : outcome
            ? t("run_state_failed")
            : t("run_state_idle");

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="page-kicker">{t("run_kicker")}</div>
          <h1 className="page-title">{t("run_title")}</h1>
          <p className="page-desc">{t("run_desc")}</p>
        </div>
        <div className="chip-row">
          <span
            className={`badge ${
              phase === "failed" || phase === "cancelled" || outcome?.cancelled
                ? "badge-bad"
                : phase === "completed" || outcome?.succeeded
                  ? "badge-good"
                  : running
                    ? "badge-detected"
                    : "badge-unknown"
            }`}
          >
            {runState}
          </span>
        </div>
      </div>

      {!status.llamaBinPath && <div className="error-banner">{t("run_need_binary")}</div>}
      {error && (
        <div className="error-banner">
          <div>{error}</div>
          <details className="details-toggle">
            <summary>{t("common_technical_details")}</summary>
            <pre>{error}</pre>
          </details>
        </div>
      )}

      <div className="run-workspace">
        <div className="panel run-controls">
          <div className="panel-title">{t("run_launch")}</div>
          <div className="field">
            <label htmlFor="run-model">{t("run_model")}</label>
            <select id="run-model" value={modelId} onChange={(e) => setModelId(e.target.value)} disabled={running}>
              <option value="">{t("run_select_model")}</option>
              {models.map((m) => (
                <option key={m.library_id} value={m.library_id}>
                  {m.alias ?? m.current_path.split(/[\\/]/).pop()}
                </option>
              ))}
            </select>
          </div>
          <div className="field">
            <label htmlFor="run-profile">{t("run_profile")}</label>
            <select
              id="run-profile"
              value={profileId}
              onChange={(e) => setProfileId(e.target.value)}
              disabled={running || profiles.length === 0}
            >
              {profiles.length === 0 && <option value="">{t("run_no_profile")}</option>}
              {profiles.map((p) => (
                <option key={p.profile_id} value={p.profile_id}>
                  {p.profile_id} ({p.backend})
                </option>
              ))}
            </select>
          </div>

          {activeProfile && (
            <div className="telemetry-strip" style={{ gridTemplateColumns: "1fr 1fr" }}>
              <div className="telemetry-item">
                <div className="stat-label">{t("status_backend")}</div>
                <div className="stat-value" style={{ fontSize: 14 }}>
                  {activeProfile.backend}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_threads")}</div>
                <div className="stat-value num" style={{ fontSize: 14 }}>
                  {activeProfile.threads}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_gpu_layers")}</div>
                <div className="stat-value num" style={{ fontSize: 14 }}>
                  {activeProfile.gpu_layers}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_context")}</div>
                <div className="stat-value num" style={{ fontSize: 14 }}>
                  {activeProfile.context_size}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_batch")}</div>
                <div className="stat-value num" style={{ fontSize: 14 }}>
                  {activeProfile.batch_size}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_measured_gen")}</div>
                <div className="stat-value num" style={{ fontSize: 14 }}>
                  {formatTokensPerSecond(activeProfile.mean_generation_tokens_per_second)}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_memory_estimate")}</div>
                <div className="stat-value num" style={{ fontSize: 14 }}>
                  {formatBytes(activeProfile.predicted_ram_bytes)}
                  {activeProfile.predicted_vram_bytes ? ` / ${formatBytes(activeProfile.predicted_vram_bytes)} VRAM` : ""}
                </div>
              </div>
            </div>
          )}

          <div className="kv-row" style={{ fontSize: 11 }}>
            <span className="kv-row-label">{t("run_binary")}</span>
            <span className="kv-row-value mono text-secondary" style={{ wordBreak: "break-all" }}>
              {status.llamaBinPath || "—"}
            </span>
          </div>

          <div className="field">
            <label htmlFor="run-prompt">{t("run_prompt")}</label>
            <textarea
              id="run-prompt"
              rows={8}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder={t("run_prompt_placeholder")}
              disabled={running}
            />
          </div>

          <div className="action-stack">
            <button className="btn btn-primary" type="button" onClick={handleRun} disabled={!ready || running || !prompt.trim()}>
              {t("run_start")}
            </button>
            {running && (
              <button className="btn btn-danger" type="button" onClick={handleStop}>
                {t("run_stop")}
              </button>
            )}
          </div>
          <p className="text-tertiary" style={{ fontSize: 11 }}>
            {t("run_no_history_note")}
          </p>
        </div>

        <div className="panel" style={{ display: "flex", flexDirection: "column", minHeight: 0 }}>
          <div className="panel-title">{t("run_output")}</div>
          <div ref={outputRef} className="run-output" aria-live="polite" aria-busy={running}>
            {output || <span className="text-tertiary">{running ? t("common_loading") : "—"}</span>}
          </div>

          {(outcome || running) && (
            <div className="telemetry-strip" style={{ marginTop: 12 }}>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_generation")}</div>
                <div className="stat-value num" style={{ fontSize: 15 }}>
                  {formatTokensPerSecond(outcome?.generation_tokens_per_second)}
                </div>
                <span className="badge badge-measured" style={{ marginTop: 4 }}>
                  {t("common_measured")}
                </span>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_prompt_tps")}</div>
                <div className="stat-value num" style={{ fontSize: 15 }}>
                  {formatTokensPerSecond(outcome?.prompt_tokens_per_second)}
                </div>
                <span className="badge badge-measured" style={{ marginTop: 4 }}>
                  {t("common_measured")}
                </span>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_elapsed")}</div>
                <div className="stat-value num" style={{ fontSize: 15 }}>
                  {outcome ? `${outcome.elapsed_secs.toFixed(1)}s` : "—"}
                </div>
              </div>
              <div className="telemetry-item">
                <div className="stat-label">{t("run_status")}</div>
                <div className="stat-value" style={{ fontSize: 15 }}>
                  {runState}
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
