import { useEffect, useState } from "react";
import { useI18n } from "../i18n/I18nContext";
import { getHardwareProfile, listCalibrations } from "../lib/api";
import type { Confidence, HardwareCapabilityProfile, CalibrationStore } from "../lib/types";
import { formatBytes } from "../lib/format";
import { ConfidenceBadge } from "../components/Badges";

function Row({
  label,
  value,
  confidence,
  tip,
}: {
  label: string;
  value: string;
  confidence?: Confidence;
  tip?: string;
}) {
  return (
    <div className="kv-row">
      <span className="kv-row-label" title={tip}>
        {label}
      </span>
      <span className="kv-row-value">
        <span className="mono num path-text">{value}</span>
        {confidence && <ConfidenceBadge confidence={confidence} />}
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
        <div>
          <div className="page-kicker">{t("hardware_kicker")}</div>
          <h1 className="page-title">{t("hardware_title")}</h1>
          <p className="page-desc">{t("hardware_desc")}</p>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {!hw && !error && <div className="loading-block">{t("common_loading")}</div>}

      {hw && (
        <>
          <div className="summary-strip" style={{ marginBottom: 20 }}>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("hardware_cpu")}</div>
              <div className="summary-strip-value">{hw.cpu.brand.value ?? "—"}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("hardware_memory")}</div>
              <div className="summary-strip-value num">{formatBytes(hw.memory.total_bytes.value)}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("hardware_gpu")}</div>
              <div className="summary-strip-value">{hw.gpus[0]?.name ?? t("common_unavailable")}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("hardware_os")}</div>
              <div className="summary-strip-value">
                {hw.os.product_name.value ?? "—"} {hw.os.display_version.value ?? ""}
              </div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("hardware_calibration")}</div>
              <div className="summary-strip-value num">{hw.calibration_record_count}</div>
            </div>
          </div>

          <div className="stack-2">
            <div className="panel">
              <div className="panel-title">{t("hardware_cpu")}</div>
              <Row label={t("hardware_brand")} value={hw.cpu.brand.value ?? "—"} confidence={hw.cpu.brand.confidence} />
              <Row label={t("hardware_vendor")} value={hw.cpu.vendor.value ?? "—"} confidence={hw.cpu.vendor.confidence} />
              <Row
                label={t("hardware_physical_cores")}
                value={String(hw.cpu.physical_cores.value ?? "—")}
                confidence={hw.cpu.physical_cores.confidence}
              />
              <Row
                label={t("hardware_logical_cores")}
                value={String(hw.cpu.logical_cores.value ?? "—")}
                confidence={hw.cpu.logical_cores.confidence}
              />
              <Row
                label={t("hardware_isa")}
                value={(hw.cpu.instruction_sets.value ?? []).join(", ") || "—"}
                confidence={hw.cpu.instruction_sets.confidence}
              />
            </div>

            <div className="panel">
              <div className="panel-title">{t("hardware_memory")}</div>
              <Row
                label={t("hardware_total")}
                value={formatBytes(hw.memory.total_bytes.value)}
                confidence={hw.memory.total_bytes.confidence}
              />
              <Row
                label={t("hardware_available")}
                value={formatBytes(hw.memory.available_bytes.value)}
                confidence={hw.memory.available_bytes.confidence}
              />
            </div>

            <div className="panel">
              <div className="panel-title">{t("hardware_gpu")}</div>
              {hw.gpus.length === 0 && <div className="text-tertiary">{t("common_unavailable")}</div>}
              {hw.gpus.map((gpu, i) => (
                <div key={i} style={{ marginBottom: i < hw.gpus.length - 1 ? 12 : 0 }}>
                  <Row label={t("hardware_name")} value={gpu.name} confidence="detected" />
                  <Row
                    label="VRAM"
                    value={formatBytes(gpu.dedicated_vram_bytes)}
                    tip={t("hardware_vram_tip")}
                    confidence={gpu.dedicated_vram_bytes != null ? "detected" : "unavailable"}
                  />
                  <Row
                    label={t("hardware_shared_mem")}
                    value={formatBytes(gpu.shared_system_memory_bytes)}
                    tip={t("hardware_shared_tip")}
                    confidence={gpu.shared_system_memory_bytes != null ? "detected" : "unavailable"}
                  />
                  <Row label={t("hardware_driver")} value={gpu.driver_version ?? "—"} confidence="detected" />
                </div>
              ))}
              <hr className="divider" />
              <Row
                label="CUDA"
                value={String(hw.backends.cuda.value ?? "—")}
                confidence={hw.backends.cuda.confidence}
                tip={t("hardware_cuda_tip")}
              />
              <Row
                label="Vulkan"
                value={String(hw.backends.vulkan.value ?? "—")}
                confidence={hw.backends.vulkan.confidence}
                tip={t("hardware_vulkan_tip")}
              />
              <p className="text-tertiary" style={{ fontSize: 11, marginTop: 8 }}>
                {t("hardware_detected_vs_verified")}
              </p>
            </div>

            <div className="panel">
              <div className="panel-title">{t("hardware_os")}</div>
              <Row
                label={t("hardware_product")}
                value={hw.os.product_name.value ?? "—"}
                confidence={hw.os.product_name.confidence}
              />
              <Row
                label={t("hardware_version")}
                value={hw.os.display_version.value ?? "—"}
                confidence={hw.os.display_version.confidence}
              />
              <Row
                label={t("hardware_build")}
                value={hw.os.build_number.value ?? "—"}
                confidence={hw.os.build_number.confidence}
              />
              <Row
                label={t("hardware_arch")}
                value={hw.os.native_architecture.value ?? "—"}
                confidence={hw.os.native_architecture.confidence}
              />
            </div>

            <div className="panel">
              <div className="panel-title">{t("hardware_storage")}</div>
              {hw.storage ? (
                <>
                  <Row
                    label={t("hardware_free")}
                    value={formatBytes(hw.storage.free_bytes.value)}
                    confidence={hw.storage.free_bytes.confidence}
                  />
                  <Row
                    label={t("hardware_total")}
                    value={formatBytes(hw.storage.total_bytes.value)}
                    confidence={hw.storage.total_bytes.confidence}
                  />
                  <div className="path-text text-tertiary" style={{ marginTop: 8, fontSize: 10.5 }}>
                    {hw.storage.path_queried}
                  </div>
                </>
              ) : (
                <div className="text-tertiary">{t("common_unavailable")}</div>
              )}
            </div>

            <div className="panel">
              <div className="panel-title">{t("hardware_power")}</div>
              <Row
                label={t("hardware_ac")}
                value={hw.power.ac_line_status.value ?? "—"}
                confidence={hw.power.ac_line_status.confidence}
              />
              <Row
                label={t("hardware_battery")}
                value={hw.power.battery_present.value ? `${hw.power.battery_percent.value ?? "?"}%` : "—"}
                confidence={hw.power.battery_present.confidence}
              />
              <Row
                label={t("hardware_chassis")}
                value={hw.power.chassis_class.value ?? "—"}
                confidence={hw.power.chassis_class.confidence}
              />
            </div>
          </div>

          <section className="section" style={{ marginTop: 20 }}>
            <div className="panel">
              <div className="panel-title">{t("hardware_calibration")}</div>
              <div className="metric-card-value num" style={{ marginBottom: 4 }}>
                {hw.calibration_record_count}
              </div>
              <p className="text-secondary">
                {calibrations
                  ? t("hardware_calibration_store").replace("{n}", String(calibrations.records.length))
                  : t("common_loading")}
              </p>
              <div className="chip-row" style={{ marginTop: 10 }}>
                <span className="badge badge-measured">
                  {t("common_measured")}: {hw.confidence_summary.measured_count}
                </span>
                <span className="badge badge-detected">
                  {t("common_detected")}: {hw.confidence_summary.detected_count}
                </span>
                <span className="badge badge-inferred">
                  {t("common_inferred")}: {hw.confidence_summary.inferred_count}
                </span>
                <span className="badge badge-unavailable">
                  {t("common_unavailable")}: {hw.confidence_summary.unavailable_count}
                </span>
              </div>
            </div>
          </section>
        </>
      )}
    </div>
  );
}
