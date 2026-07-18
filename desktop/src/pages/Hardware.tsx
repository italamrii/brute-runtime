import { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nContext";
import { getHardwareProfile, listCalibrations } from "../lib/api";
import type { HardwareCapabilityProfile, CalibrationStore } from "../lib/types";
import { formatBytes } from "../lib/format";
import { ConfidenceBadge } from "../components/Badges";

function Row({ label, value, confidence, tip }: { label: string; value: string; confidence?: string; tip?: string }) {
  return (
    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "8px 0", borderBottom: "1px solid var(--border-subtle)" }}>
      <span className="text-secondary" title={tip}>
        {label}
      </span>
      <span style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <span className="mono">{value}</span>
        {confidence && <ConfidenceBadge confidence={confidence as never} />}
      </span>
    </div>
  );
}

export function Hardware() {
  const { t } = useI18n();
  const [hw, setHw] = useState<HardwareCapabilityProfile | null>(null);
  const [calibrations, setCalibrations] = useState<CalibrationStore | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([getHardwareProfile(), listCalibrations()])
      .then(([h, c]) => {
        setHw(h);
        setCalibrations(c);
      })
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="page">
      <div className="page-header">
        <h1 className="page-title">{t("hardware_title")}</h1>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {!hw && !error && <div className="text-tertiary">{t("common_loading")}</div>}

      {hw && (
        <div className="grid-cards" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(320px, 1fr))" }}>
          <div className="panel">
            <div className="panel-title">{t("hardware_cpu")}</div>
            <Row label="Brand" value={hw.cpu.brand.value ?? "—"} confidence={hw.cpu.brand.confidence} />
            <Row label="Vendor" value={hw.cpu.vendor.value ?? "—"} confidence={hw.cpu.vendor.confidence} />
            <Row label="Physical cores" value={String(hw.cpu.physical_cores.value ?? "—")} confidence={hw.cpu.physical_cores.confidence} />
            <Row label="Logical cores" value={String(hw.cpu.logical_cores.value ?? "—")} confidence={hw.cpu.logical_cores.confidence} />
            <Row label="ISA" value={(hw.cpu.instruction_sets.value ?? []).join(", ") || "—"} confidence={hw.cpu.instruction_sets.confidence} />
          </div>

          <div className="panel">
            <div className="panel-title">{t("hardware_memory")}</div>
            <Row label="Total" value={formatBytes(hw.memory.total_bytes.value)} confidence={hw.memory.total_bytes.confidence} />
            <Row label="Available" value={formatBytes(hw.memory.available_bytes.value)} confidence={hw.memory.available_bytes.confidence} />
          </div>

          <div className="panel">
            <div className="panel-title">{t("hardware_gpu")}</div>
            {hw.gpus.length === 0 && <div className="text-tertiary">{t("common_unavailable")}</div>}
            {hw.gpus.map((gpu, i) => (
              <div key={i} style={{ marginBottom: 8 }}>
                <Row label="Name" value={gpu.name} />
                <Row label="VRAM" value={formatBytes(gpu.dedicated_vram_bytes)} tip={t("hardware_vram_tip")} />
                <Row label="Driver" value={gpu.driver_version ?? "—"} />
              </div>
            ))}
            <Row label="CUDA" value={String(hw.backends.cuda.value ?? "—")} confidence={hw.backends.cuda.confidence} tip={t("hardware_cuda_tip")} />
            <Row label="Vulkan" value={String(hw.backends.vulkan.value ?? "—")} confidence={hw.backends.vulkan.confidence} tip={t("hardware_vulkan_tip")} />
          </div>

          <div className="panel">
            <div className="panel-title">{t("hardware_os")}</div>
            <Row label="Product" value={hw.os.product_name.value ?? "—"} confidence={hw.os.product_name.confidence} />
            <Row label="Version" value={hw.os.display_version.value ?? "—"} confidence={hw.os.display_version.confidence} />
            <Row label="Build" value={hw.os.build_number.value ?? "—"} confidence={hw.os.build_number.confidence} />
            <Row label="Architecture" value={hw.os.native_architecture.value ?? "—"} confidence={hw.os.native_architecture.confidence} />
          </div>

          <div className="panel">
            <div className="panel-title">{t("hardware_storage")}</div>
            {hw.storage ? (
              <>
                <Row label="Free" value={formatBytes(hw.storage.free_bytes.value)} confidence={hw.storage.free_bytes.confidence} />
                <Row label="Total" value={formatBytes(hw.storage.total_bytes.value)} confidence={hw.storage.total_bytes.confidence} />
              </>
            ) : (
              <div className="text-tertiary">{t("common_unavailable")}</div>
            )}
          </div>

          <div className="panel">
            <div className="panel-title">{t("hardware_power")}</div>
            <Row label="AC status" value={hw.power.ac_line_status.value ?? "—"} confidence={hw.power.ac_line_status.confidence} />
            <Row label="Battery" value={hw.power.battery_present.value ? `${hw.power.battery_percent.value ?? "?"}%` : "—"} confidence={hw.power.battery_present.confidence} />
            <Row label="Chassis" value={hw.power.chassis_class.value ?? "—"} confidence={hw.power.chassis_class.confidence} />
          </div>

          <div className="panel">
            <div className="panel-title">{t("hardware_calibration")}</div>
            <div className="stat-value">{hw.calibration_record_count}</div>
            <div className="text-tertiary">{calibrations ? `${calibrations.records.length} total in store` : ""}</div>
          </div>
        </div>
      )}
    </div>
  );
}
