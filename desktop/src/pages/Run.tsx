import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useI18n } from "../i18n/I18nContext";
import { getAssociations, listLibrary, localRunCancel, localRunGenerate } from "../lib/api";
import type { LibraryEntry, LocalRunOutcome, RuntimeProfile } from "../lib/types";
import { formatTokensPerSecond } from "../lib/format";
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
  const [error, setError] = useState<string | null>(null);
  const outputRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    listLibrary().then(setModels).catch(() => setModels([]));
  }, []);

  useEffect(() => {
    if (!modelId) {
      setProfiles([]);
      setProfileId("");
      return;
    }
    getAssociations(modelId)
      .then((a) => {
        setProfiles(a.runtime_profiles);
        setProfileId(a.runtime_profiles[0]?.profile_id ?? "");
      })
      .catch(() => setProfiles([]));
  }, [modelId]);

  useEffect(() => {
    const unlisten = listen<string>("local-run-chunk", (event) => {
      setOutput((prev) => prev + event.payload);
    });
    return () => {
      unlisten.then((f) => f());
    };
  }, []);

  useEffect(() => {
    outputRef.current?.scrollTo({ top: outputRef.current.scrollHeight });
  }, [output]);

  async function handleRun() {
    if (!modelId || !profileId || !status.llamaBinPath) return;
    setRunning(true);
    setError(null);
    setOutcome(null);
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

  const ready = modelId && profileId && status.llamaBinPath;

  return (
    <div className="page">
      <div className="page-header">
        <h1 className="page-title">{t("run_title")}</h1>
      </div>

      {!status.llamaBinPath && <div className="error-banner">Set a runtime binary path in Settings before running a model.</div>}

      <div style={{ display: "flex", gap: 12, marginBottom: 12, flexWrap: "wrap" }}>
        <div className="field" style={{ minWidth: 240 }}>
          <label>Model</label>
          <select value={modelId} onChange={(e) => setModelId(e.target.value)} disabled={running}>
            <option value="">Select a model…</option>
            {models.map((m) => (
              <option key={m.library_id} value={m.library_id}>
                {m.alias ?? m.current_path.split(/[\\/]/).pop()}
              </option>
            ))}
          </select>
        </div>
        <div className="field" style={{ minWidth: 240 }}>
          <label>Runtime profile</label>
          <select value={profileId} onChange={(e) => setProfileId(e.target.value)} disabled={running || profiles.length === 0}>
            {profiles.length === 0 && <option value="">No saved profile for this model — tune it first</option>}
            {profiles.map((p) => (
              <option key={p.profile_id} value={p.profile_id}>
                {p.profile_id} ({p.backend})
              </option>
            ))}
          </select>
        </div>
      </div>

      <div className="field">
        <label htmlFor="run-prompt">Prompt</label>
        <textarea
          id="run-prompt"
          rows={4}
          value={prompt}
          onChange={(e) => setPrompt(e.target.value)}
          placeholder={t("run_prompt_placeholder")}
          disabled={running}
        />
      </div>

      <div style={{ display: "flex", gap: 8, marginBottom: 12 }}>
        <button className="btn btn-primary" onClick={handleRun} disabled={!ready || running || !prompt.trim()}>
          {t("run_start")}
        </button>
        {running && (
          <button className="btn btn-danger" onClick={handleStop}>
            {t("run_stop")}
          </button>
        )}
      </div>

      {error && <div className="error-banner">{error}</div>}

      <div className="panel" style={{ marginBottom: 12 }}>
        <div className="panel-title">Output</div>
        <div
          ref={outputRef}
          className="mono"
          style={{ whiteSpace: "pre-wrap", minHeight: 160, maxHeight: 360, overflowY: "auto" }}
        >
          {output || <span className="text-tertiary">{running ? t("common_loading") : "—"}</span>}
        </div>
      </div>

      {outcome && (
        <div className="grid-cards">
          <div className="stat">
            <div className="stat-label">Generation</div>
            <div className="stat-value">{formatTokensPerSecond(outcome.generation_tokens_per_second)}</div>
          </div>
          <div className="stat">
            <div className="stat-label">Prompt</div>
            <div className="stat-value">{formatTokensPerSecond(outcome.prompt_tokens_per_second)}</div>
          </div>
          <div className="stat">
            <div className="stat-label">Elapsed</div>
            <div className="stat-value">{outcome.elapsed_secs.toFixed(1)}s</div>
          </div>
          <div className="stat">
            <div className="stat-label">Status</div>
            <div className="stat-value">{outcome.cancelled ? "Cancelled" : outcome.timed_out ? "Timed out" : outcome.succeeded ? "Completed" : "Failed"}</div>
          </div>
        </div>
      )}

      <p className="text-tertiary" style={{ marginTop: 16 }}>
        {t("run_no_history_note")}
      </p>
    </div>
  );
}
