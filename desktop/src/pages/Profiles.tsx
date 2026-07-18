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
        <h1 className="page-title">{t("profiles_title")}</h1>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {ids.length === 0 && <div className="empty-state">{t("profiles_empty")}</div>}

      <div style={{ display: "grid", gridTemplateColumns: selected ? "260px 1fr" : "1fr", gap: 16 }}>
        <div className="panel">
          <ul style={{ listStyle: "none", margin: 0, padding: 0, display: "flex", flexDirection: "column", gap: 4 }}>
            {ids.map((id) => (
              <li key={id}>
                <button
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
          <div className="panel">
            <h3>{selected.profile_id}</h3>
            <p className="text-tertiary">Tuned {selected.tuning_date}</p>
            <dl style={{ margin: "12px 0" }}>
              {[
                ["Backend", selected.backend],
                ["Threads", String(selected.threads)],
                ["GPU layers", String(selected.gpu_layers)],
                ["Context", String(selected.context_size)],
                ["Batch", String(selected.batch_size)],
                ["Generation", formatTokensPerSecond(selected.mean_generation_tokens_per_second)],
                ["Prompt", formatTokensPerSecond(selected.mean_prompt_tokens_per_second)],
                ["Stability", selected.stability],
                ["Confidence", selected.confidence],
                ["Repetitions", `${selected.source_repetitions_succeeded}/${selected.source_repetitions_requested}`],
              ].map(([label, value]) => (
                <div key={label} style={{ display: "flex", justifyContent: "space-between", padding: "4px 0", borderBottom: "1px solid var(--border-subtle)" }}>
                  <span className="text-secondary">{label}</span>
                  <span className="mono">{value}</span>
                </div>
              ))}
            </dl>

            {!modelForProfile && <p className="text-tertiary">Model for this profile is not currently in your library.</p>}

            <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
              <button className="btn" onClick={handleVerify} disabled={busy || !modelForProfile || !status.llamaBinPath}>
                {t("common_verify")}
              </button>
              <button className="btn" onClick={handleExport} disabled={busy}>
                {t("common_export")}
              </button>
              <button
                className="btn"
                onClick={() => {
                  status.setActiveProfile(selected.profile_id, selected.backend);
                }}
                disabled={busy}
              >
                Set as active
              </button>
              <button className="btn btn-danger" onClick={handleDelete} disabled={busy}>
                {t("common_delete")}
              </button>
            </div>

            {applyResult && (
              <div className="panel" style={{ marginTop: 12, borderColor: applyResult.status === "verified" ? "var(--state-good)" : "var(--state-bad)" }}>
                <p>
                  Status: <strong>{applyResult.status}</strong>
                </p>
                <p className="text-secondary">{applyResult.detail}</p>
                {applyResult.rollback_recommended && <p className="text-tertiary">Rollback recommended — consider re-tuning.</p>}
                {applyResult.compatibility_issues.map((issue) => (
                  <p key={issue} className="text-tertiary">
                    {issue}
                  </p>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
