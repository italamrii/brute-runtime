import { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { auditLibrary, getHardwareProfile, getStorageSummary, listLibrary, listProfiles, recommendModel } from "../lib/api";
import type { AuditReport, HardwareCapabilityProfile, LibraryEntry, Recommendation, StorageSummary } from "../lib/types";
import { formatBytes } from "../lib/format";
import { ConfidenceBadge } from "../components/Badges";
import { IconShield, IconTrophy } from "../components/Icons";
import type { Page } from "../App";

export function Overview({ onNavigate }: { onNavigate: (page: Page) => void }) {
  const { t } = useI18n();
  const status = useAppStatus();
  const [hw, setHw] = useState<HardwareCapabilityProfile | null>(null);
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [profiles, setProfiles] = useState<string[] | null>(null);
  const [storage, setStorage] = useState<StorageSummary | null>(null);
  const [audit, setAudit] = useState<AuditReport | null>(null);
  const [recommendation, setRecommendation] = useState<Recommendation | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      getHardwareProfile(),
      listLibrary(),
      listProfiles(),
      getStorageSummary(),
      auditLibrary(),
      recommendModel(null, "balanced").catch(() => null),
    ])
      .then(([h, e, p, s, a, rec]) => {
        setHw(h);
        setEntries(e);
        setProfiles(p);
        setStorage(s);
        setAudit(a);
        setRecommendation(rec?.recommendation ?? null);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const quarantined = entries?.filter((e) => e.quarantine !== null).length ?? 0;
  const healthy = audit?.healthy.length ?? null;
  const problemCount =
    (audit?.missing.length ?? 0) +
    (audit?.modified.length ?? 0) +
    (audit?.corrupt.length ?? 0) +
    quarantined;

  const ramTotal = hw?.memory.total_bytes.value ?? null;
  const ramAvail = hw?.memory.available_bytes.value ?? null;
  const ramUsed = ramTotal !== null && ramAvail !== null ? Math.max(0, ramTotal - ramAvail) : null;
  const ramPct = ramTotal && ramUsed !== null ? Math.min(100, Math.round((ramUsed / ramTotal) * 100)) : null;

  const diskTotal = hw?.storage?.total_bytes.value ?? null;
  const diskFree = hw?.storage?.free_bytes.value ?? null;
  const libBytes = storage?.total_bytes ?? null;
  const diskUsedPct =
    diskTotal && diskFree !== null ? Math.min(100, Math.round(((diskTotal - diskFree) / diskTotal) * 100)) : null;

  const libraryHealthLabel =
    audit == null
      ? t("common_loading")
      : problemCount === 0
        ? t("overview_library_healthy")
        : t("overview_library_attention").replace("{n}", String(problemCount));

  const fitTone =
    recommendation?.recommended.fit.state === "excellent" || recommendation?.recommended.fit.state === "good"
      ? "good"
      : recommendation?.recommended.fit.state === "not_recommended"
        ? "bad"
        : "warn";

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="page-kicker">{t("overview_kicker")}</div>
          <h1 className="page-title">{t("overview_title")}</h1>
          <p className="page-desc">{t("overview_desc")}</p>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}

      <div className="summary-strip" role="region" aria-label={t("overview_ops_strip")}>
        <div className="summary-strip-item">
          <div className="summary-strip-label">
            <IconShield size={12} />
            {t("overview_ops_mode")}
          </div>
          <div className="summary-strip-value" style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span className="status-dot" aria-hidden="true" />
            {t("status_offline")}
          </div>
        </div>
        <div className="summary-strip-item">
          <div className="summary-strip-label">{t("status_active_model")}</div>
          <div className="summary-strip-value" title={status.libraryLabel ?? undefined}>
            {status.libraryLabel ?? t("status_none")}
          </div>
        </div>
        <div className="summary-strip-item">
          <div className="summary-strip-label">{t("status_active_profile")}</div>
          <div className="summary-strip-value">{status.profileId ?? t("status_none")}</div>
        </div>
        <div className="summary-strip-item">
          <div className="summary-strip-label">{t("status_backend")}</div>
          <div className="summary-strip-value">{status.backend ?? t("status_none")}</div>
        </div>
        <div className="summary-strip-item">
          <div className="summary-strip-label">{t("overview_library_health")}</div>
          <div className="summary-strip-value" style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <span className={`status-dot ${problemCount > 0 ? "is-warn" : ""}`} aria-hidden="true" />
            {libraryHealthLabel}
          </div>
        </div>
      </div>

      <section className="section" aria-labelledby="hw-ready-title">
        <div className="section-header">
          <h2 id="hw-ready-title" className="section-title">
            {t("overview_device_readiness")}
          </h2>
          <button type="button" className="btn btn-sm btn-ghost" onClick={() => onNavigate("hardware")}>
            {t("overview_inspect_hardware")}
          </button>
        </div>
        <div className="metric-row">
          <div className="metric-card">
            <div className="metric-card-header">
              <span className="metric-card-label">{t("overview_cpu")}</span>
              {hw && <ConfidenceBadge confidence={hw.cpu.brand.confidence} />}
            </div>
            <div className="metric-card-value" title={hw?.cpu.brand.value ?? undefined}>
              {hw ? (hw.cpu.brand.value ?? t("common_unknown")) : "—"}
            </div>
            <div className="metric-card-sub">
              {hw
                ? `${hw.cpu.physical_cores.value ?? "?"} ${t("overview_cores")} / ${hw.cpu.logical_cores.value ?? "?"} ${t("overview_threads")}`
                : t("common_loading")}
            </div>
          </div>

          <div className="metric-card">
            <div className="metric-card-header">
              <span className="metric-card-label">{t("overview_gpu")}</span>
              {hw?.gpus[0] && <ConfidenceBadge confidence="detected" />}
            </div>
            <div className="metric-card-value" title={hw?.gpus[0]?.name}>
              {hw ? (hw.gpus[0]?.name ?? t("common_unavailable")) : "—"}
            </div>
            <div className="metric-card-sub">
              {hw?.gpus[0]
                ? `${formatBytes(hw.gpus[0].dedicated_vram_bytes)} VRAM`
                : hw
                  ? t("common_unavailable")
                  : t("common_loading")}
            </div>
          </div>

          <div className="metric-card">
            <div className="metric-card-header">
              <span className="metric-card-label">{t("overview_ram")}</span>
              {hw && <ConfidenceBadge confidence={hw.memory.total_bytes.confidence} />}
            </div>
            <div className="metric-card-value num">{hw ? formatBytes(ramTotal) : "—"}</div>
            <div className="metric-card-sub">
              {ramUsed !== null
                ? `${formatBytes(ramUsed)} ${t("overview_in_use")} · ${formatBytes(ramAvail)} ${t("overview_available")}`
                : t("common_loading")}
            </div>
            {ramPct !== null && (
              <div className="meter" role="meter" aria-valuenow={ramPct} aria-valuemin={0} aria-valuemax={100} aria-label={t("overview_ram")}>
                <div className="meter-fill" style={{ width: `${ramPct}%` }} />
              </div>
            )}
          </div>

          <div className="metric-card">
            <div className="metric-card-header">
              <span className="metric-card-label">{t("overview_backends_verified")}</span>
            </div>
            <div className="backend-list">
              <div className="backend-row">
                <span>llama.cpp / CPU</span>
                <ConfidenceBadge confidence={hw ? "detected" : "unavailable"} />
              </div>
              <div className="backend-row">
                <span>CUDA</span>
                {hw ? <ConfidenceBadge confidence={hw.backends.cuda.confidence} /> : <span className="text-tertiary">—</span>}
              </div>
              <div className="backend-row">
                <span>Vulkan</span>
                {hw ? <ConfidenceBadge confidence={hw.backends.vulkan.confidence} /> : <span className="text-tertiary">—</span>}
              </div>
            </div>
            <p className="text-tertiary" style={{ fontSize: 10.5, marginTop: 4 }}>
              {t("overview_backend_note")}
            </p>
          </div>
        </div>
      </section>

      <section className="section" aria-labelledby="local-ai-title">
        <div className="section-header">
          <h2 id="local-ai-title" className="section-title">
            {t("overview_local_ai")}
          </h2>
        </div>
        <div className="metric-row">
          <button type="button" className="metric-card is-actionable" onClick={() => onNavigate("models")}>
            <div className="metric-card-label">{t("overview_models_total")}</div>
            <div className="metric-card-value num">{entries?.length ?? "—"}</div>
            <div className="metric-card-foot">
              <span className="badge badge-measured">{t("common_measured")}</span>
            </div>
          </button>
          <button type="button" className="metric-card is-actionable" onClick={() => onNavigate("health")}>
            <div className="metric-card-label">{t("overview_models_healthy")}</div>
            <div className="metric-card-value num">{healthy ?? "—"}</div>
            <div className="metric-card-foot">
              <span className="badge badge-measured">{t("common_measured")}</span>
            </div>
          </button>
          <button type="button" className="metric-card is-actionable" onClick={() => onNavigate("health")}>
            <div className="metric-card-label">{t("overview_models_quarantined")}</div>
            <div className="metric-card-value num">{entries ? quarantined : "—"}</div>
            <div className="metric-card-foot">
              <span className="badge badge-measured">{t("common_measured")}</span>
            </div>
          </button>
          <button type="button" className="metric-card is-actionable" onClick={() => onNavigate("profiles")}>
            <div className="metric-card-label">{t("overview_profiles_saved")}</div>
            <div className="metric-card-value num">{profiles?.length ?? "—"}</div>
            <div className="metric-card-foot">
              <span className="badge badge-measured">{t("common_measured")}</span>
            </div>
          </button>
          <div className="metric-card">
            <div className="metric-card-label">{t("overview_storage")}</div>
            <div className="metric-card-value num">{libBytes != null ? formatBytes(libBytes) : "—"}</div>
            <div className="metric-card-sub">
              {diskTotal != null
                ? `${formatBytes(diskFree)} ${t("overview_free_of")} ${formatBytes(diskTotal)}`
                : t("overview_library_bytes")}
            </div>
            {diskUsedPct !== null && (
              <div className="meter" role="meter" aria-valuenow={diskUsedPct} aria-valuemin={0} aria-valuemax={100}>
                <div className="meter-fill" style={{ width: `${diskUsedPct}%` }} />
              </div>
            )}
            <div className="metric-card-foot">
              <span className="badge badge-measured">{t("common_measured")}</span>
            </div>
          </div>
        </div>
      </section>

      <div className="stack-3 section">
        <div className="recommend-card">
          <div className="recommend-card-head">
            <div>
              <div className="metric-card-label">{t("overview_best_model")}</div>
              <h3 style={{ fontSize: 15, marginTop: 4 }}>
                {recommendation?.recommended.build.display_name ?? t("overview_no_recommendation")}
              </h3>
            </div>
            <div className="recommend-trophy" aria-hidden="true">
              <IconTrophy size={14} />
            </div>
          </div>
          {recommendation ? (
            <>
              <div className="chip-row">
                <span className={`badge badge-${fitTone}`}>{recommendation.recommended.fit.state}</span>
                <span className="badge badge-inferred">{t("common_inferred")}</span>
              </div>
              <p className="text-secondary" style={{ fontSize: 12 }}>
                {recommendation.explanation.simple}
              </p>
              {recommendation.safer_fallback && (
                <p className="text-tertiary" style={{ fontSize: 11 }}>
                  {t("overview_safer_fallback")}: {recommendation.safer_fallback.build.display_name}
                </p>
              )}
              <button type="button" className="btn btn-sm" onClick={() => onNavigate("optimize")}>
                {t("overview_open_optimize")}
              </button>
            </>
          ) : (
            <p className="text-tertiary">{t("overview_recommendation_hint")}</p>
          )}
        </div>

        <div className="panel">
          <div className="panel-title">{t("overview_quick_status")}</div>
          <ul className="quick-status-list">
            <li>
              <span>{t("overview_privacy")}</span>
              <span style={{ display: "flex", alignItems: "center", gap: 6 }}>
                <span className="status-dot" aria-hidden="true" />
                {t("overview_privacy_value")}
              </span>
            </li>
            <li>
              <span>{t("overview_models_healthy")}</span>
              <span className="num">{healthy ?? "—"}</span>
            </li>
            <li>
              <span>{t("overview_models_quarantined")}</span>
              <span className="num">{entries ? quarantined : "—"}</span>
            </li>
            <li>
              <span>{t("overview_runtime_binary")}</span>
              <span>{status.llamaBinPath ? t("overview_binary_set") : t("overview_binary_missing")}</span>
            </li>
          </ul>
        </div>

        <div className="panel">
          <div className="panel-title">{t("overview_quick_actions")}</div>
          <div className="action-stack">
            <button type="button" className="btn btn-primary" onClick={() => onNavigate("run")}>
              {t("overview_action_run")}
            </button>
            <button type="button" className="btn" onClick={() => onNavigate("models")}>
              {t("overview_action_import")}
            </button>
            <button type="button" className="btn" onClick={() => onNavigate("optimize")}>
              {t("overview_action_optimize")}
            </button>
            <button type="button" className="btn" onClick={() => onNavigate("health")}>
              {t("overview_action_audit")}
            </button>
            <button type="button" className="btn" onClick={() => onNavigate("settings")}>
              {t("nav_settings")}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
