import { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nContext";
import { auditLibrary, getHardwareProfile, getStorageSummary, listLibrary, listProfiles } from "../lib/api";
import type { AuditReport, HardwareCapabilityProfile, LibraryEntry, StorageSummary } from "../lib/types";
import { formatBytes } from "../lib/format";
import { ConfidenceBadge } from "../components/Badges";
import type { Page } from "../App";

export function Overview({ onNavigate }: { onNavigate: (page: Page) => void }) {
  const { t } = useI18n();
  const [hw, setHw] = useState<HardwareCapabilityProfile | null>(null);
  const [entries, setEntries] = useState<LibraryEntry[] | null>(null);
  const [profiles, setProfiles] = useState<string[] | null>(null);
  const [storage, setStorage] = useState<StorageSummary | null>(null);
  const [audit, setAudit] = useState<AuditReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([getHardwareProfile(), listLibrary(), listProfiles(), getStorageSummary(), auditLibrary()])
      .then(([h, e, p, s, a]) => {
        setHw(h);
        setEntries(e);
        setProfiles(p);
        setStorage(s);
        setAudit(a);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const quarantined = entries?.filter((e) => e.quarantine !== null).length ?? 0;
  const healthy = audit?.healthy.length ?? null;

  return (
    <div className="page">
      <div className="page-header">
        <h1 className="page-title">{t("overview_title")}</h1>
      </div>

      {error && <div className="error-banner">{error}</div>}

      <div className="grid-cards">
        <div className="panel">
          <div className="panel-title">{t("overview_cpu")}</div>
          {hw ? (
            <div className="stat">
              <div className="stat-value">{hw.cpu.brand.value ?? t("common_unknown")}</div>
              <div className="text-secondary">
                {hw.cpu.physical_cores.value ?? "?"} cores / {hw.cpu.logical_cores.value ?? "?"} threads
              </div>
              <ConfidenceBadge confidence={hw.cpu.brand.confidence} />
            </div>
          ) : (
            <div className="text-tertiary">{t("common_loading")}</div>
          )}
        </div>

        <div className="panel">
          <div className="panel-title">{t("overview_gpu")}</div>
          {hw ? (
            hw.gpus.length > 0 ? (
              <div className="stat">
                <div className="stat-value">{hw.gpus[0].name}</div>
                <div className="text-secondary">{formatBytes(hw.gpus[0].dedicated_vram_bytes)} VRAM</div>
                <div style={{ display: "flex", gap: 6 }}>
                  <span className="text-tertiary">CUDA</span>
                  <ConfidenceBadge confidence={hw.backends.cuda.confidence} />
                  <span className="text-tertiary">Vulkan</span>
                  <ConfidenceBadge confidence={hw.backends.vulkan.confidence} />
                </div>
              </div>
            ) : (
              <div className="text-tertiary">{t("common_unavailable")}</div>
            )
          ) : (
            <div className="text-tertiary">{t("common_loading")}</div>
          )}
        </div>

        <div className="panel">
          <div className="panel-title">{t("overview_ram")}</div>
          {hw ? (
            <div className="stat">
              <div className="stat-value">{formatBytes(hw.memory.total_bytes.value)}</div>
              <div className="text-secondary">{formatBytes(hw.memory.available_bytes.value)} available</div>
              <ConfidenceBadge confidence={hw.memory.total_bytes.confidence} />
            </div>
          ) : (
            <div className="text-tertiary">{t("common_loading")}</div>
          )}
        </div>

        <button className="panel" style={{ textAlign: "start", cursor: "pointer" }} onClick={() => onNavigate("models")}>
          <div className="panel-title">{t("overview_models_total")}</div>
          <div className="stat-value">{entries?.length ?? "—"}</div>
          <div className="text-secondary">
            {t("overview_models_healthy")}: {healthy ?? "—"} · {t("overview_models_quarantined")}: {quarantined}
          </div>
        </button>

        <button className="panel" style={{ textAlign: "start", cursor: "pointer" }} onClick={() => onNavigate("profiles")}>
          <div className="panel-title">{t("overview_profiles_saved")}</div>
          <div className="stat-value">{profiles?.length ?? "—"}</div>
        </button>

        <div className="panel">
          <div className="panel-title">{t("overview_storage")}</div>
          <div className="stat-value">{storage ? formatBytes(storage.total_bytes) : "—"}</div>
        </div>

        <div className="panel">
          <div className="panel-title">{t("overview_privacy")}</div>
          <div className="text-secondary">{t("overview_privacy_value")}</div>
        </div>
      </div>
    </div>
  );
}
