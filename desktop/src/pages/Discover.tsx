import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useI18n } from "../i18n/I18nContext";
import { evaluateFit, listCatalog } from "../lib/api";
import type { BuildEvaluation, FitState, ModelBuild, TaskCategory } from "../lib/types";
import { formatBytes } from "../lib/format";
import { checkUrlSafety } from "../lib/urlSafety";
import { Modal } from "../components/Modal";

interface CatalogRow {
  build: ModelBuild;
  evaluation: BuildEvaluation | null;
}

function fitStatusTone(state: FitState | undefined): "good" | "warn" | "bad" | "unknown" {
  switch (state) {
    case "excellent":
    case "good":
      return "good";
    case "constrained":
      return "warn";
    case "experimental":
      return "warn";
    case "not_recommended":
      return "bad";
    default:
      return "unknown";
  }
}

function statusLabelKey(state: FitState | undefined): string {
  switch (state) {
    case "excellent":
    case "good":
      return "discover_status_recommended";
    case "constrained":
      return "discover_status_compatible";
    case "experimental":
      return "discover_status_heavy";
    case "not_recommended":
      return "discover_status_not_recommended";
    default:
      return "common_unknown";
  }
}

interface Filters {
  arabic: boolean;
  coding: boolean;
  generalUse: boolean;
  verifiedSourceOnly: boolean;
}

export function Discover() {
  const { t } = useI18n();
  const [builds, setBuilds] = useState<ModelBuild[] | null>(null);
  const [evaluations, setEvaluations] = useState<Record<string, BuildEvaluation>>({});
  const [error, setError] = useState<string | null>(null);
  const [filters, setFilters] = useState<Filters>({
    arabic: false,
    coding: false,
    generalUse: false,
    verifiedSourceOnly: false,
  });
  const [selected, setSelected] = useState<string | null>(null);
  const [modalView, setModalView] = useState<"details" | "confirm-open" | "rejected">("details");

  function closeModal() {
    setSelected(null);
    setModalView("details");
  }

  function openSelected(catalogId: string) {
    setSelected(catalogId);
    setModalView("details");
  }

  useEffect(() => {
    listCatalog()
      .then(async (list) => {
        setBuilds(list);
        const pairs = await Promise.all(
          list.map(async (b) => {
            try {
              return [b.catalog_id, await evaluateFit(b.catalog_id)] as const;
            } catch {
              return [b.catalog_id, null] as const;
            }
          }),
        );
        const map: Record<string, BuildEvaluation> = {};
        for (const [id, evaluation] of pairs) {
          if (evaluation) map[id] = evaluation;
        }
        setEvaluations(map);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const rows: CatalogRow[] = useMemo(() => {
    if (!builds) return [];
    return builds
      .map((build) => ({ build, evaluation: evaluations[build.catalog_id] ?? null }))
      .filter(({ build }) => {
        if (filters.arabic && !build.task_categories.includes("arabic_chat")) return false;
        if (filters.coding && !build.task_categories.includes("coding")) return false;
        if (filters.generalUse && !build.task_categories.includes("general_chat")) return false;
        if (filters.verifiedSourceOnly && build.license === "unknown") return false;
        return true;
      })
      .sort((a, b) => {
        const rank = (s: FitState | undefined) =>
          ({ excellent: 0, good: 1, constrained: 2, experimental: 3, not_recommended: 4, unknown: 5 })[s ?? "unknown"];
        return rank(a.evaluation?.fit.state) - rank(b.evaluation?.fit.state);
      });
  }, [builds, evaluations, filters]);

  const selectedRow = rows.find((r) => r.build.catalog_id === selected) ?? null;

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="page-kicker">{t("discover_kicker")}</div>
          <h1 className="page-title">{t("discover_title")}</h1>
          <p className="page-desc">{t("discover_desc")}</p>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {!builds && !error && <div className="loading-block">{t("common_loading")}</div>}

      {builds && (
        <>
          <div className="filter-row" style={{ display: "flex", gap: 10, flexWrap: "wrap", marginBottom: 16 }}>
            <label className="chip-toggle">
              <input type="checkbox" checked={filters.arabic} onChange={(e) => setFilters((f) => ({ ...f, arabic: e.target.checked }))} />
              {t("discover_filter_arabic")}
            </label>
            <label className="chip-toggle">
              <input type="checkbox" checked={filters.coding} onChange={(e) => setFilters((f) => ({ ...f, coding: e.target.checked }))} />
              {t("discover_filter_coding")}
            </label>
            <label className="chip-toggle">
              <input
                type="checkbox"
                checked={filters.generalUse}
                onChange={(e) => setFilters((f) => ({ ...f, generalUse: e.target.checked }))}
              />
              {t("discover_filter_general")}
            </label>
            <label className="chip-toggle">
              <input
                type="checkbox"
                checked={filters.verifiedSourceOnly}
                onChange={(e) => setFilters((f) => ({ ...f, verifiedSourceOnly: e.target.checked }))}
              />
              {t("discover_filter_verified")}
            </label>
          </div>

          {rows.length === 0 && <div className="empty-state">{t("discover_empty")}</div>}

          <div className="metric-row" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(260px, 1fr))" }}>
            {rows.map(({ build, evaluation }) => (
              <button
                key={build.catalog_id}
                type="button"
                className="metric-card is-actionable"
                onClick={() => openSelected(build.catalog_id)}
                style={{ textAlign: "start" }}
              >
                <div className="metric-card-header">
                  <span className="metric-card-label">{build.publisher}</span>
                  <span className={`badge badge-${fitStatusTone(evaluation?.fit.state)}`}>{t(statusLabelKey(evaluation?.fit.state))}</span>
                </div>
                <div className="metric-card-value" style={{ fontSize: 15 }}>
                  {build.display_name}
                </div>
                <div className="metric-card-sub">
                  {build.parameter_count.toLocaleString()} · {build.quantization} · {formatBytes(build.file_size_bytes)}
                </div>
                <div className="metric-card-foot">
                  {build.task_categories.map((tc: TaskCategory) => (
                    <span key={tc} className="badge badge-unknown">
                      {t(`task_${tc}`)}
                    </span>
                  ))}
                </div>
              </button>
            ))}
          </div>

          {selectedRow && (
            <Modal
              open={selected !== null}
              titleId="discover-modal-title"
              title={modalView === "confirm-open" ? t("discover_confirm_open_title") : t("discover_details_title")}
              onClose={closeModal}
            >
              {modalView === "details" && (
                <>
                  <h3 style={{ marginBottom: 4 }}>{selectedRow.build.display_name}</h3>
                  <p className="text-tertiary" style={{ marginBottom: 12 }}>
                    {selectedRow.build.publisher}
                  </p>
                  <p className="text-secondary" style={{ marginBottom: 12 }}>
                    {selectedRow.build.short_description}
                  </p>

                  <dl style={{ margin: 0 }}>
                    {[
                      [t("discover_field_params"), selectedRow.build.parameter_count.toLocaleString()],
                      [t("discover_field_quant"), selectedRow.build.quantization],
                      [t("discover_field_size"), formatBytes(selectedRow.build.file_size_bytes)],
                      [
                        t("discover_field_ram"),
                        selectedRow.evaluation ? formatBytes(selectedRow.evaluation.estimate.estimated_total_ram_bytes_low) : "—",
                      ],
                      [
                        t("discover_field_vram"),
                        selectedRow.evaluation?.estimate.estimated_vram_bytes_low != null
                          ? formatBytes(selectedRow.evaluation.estimate.estimated_vram_bytes_low)
                          : t("common_unknown"),
                      ],
                      [
                        t("discover_field_license"),
                        selectedRow.build.license === "unknown" ? t("common_unknown") : selectedRow.build.license.known.identifier,
                      ],
                      [
                        t("discover_field_commercial"),
                        selectedRow.build.commercial_use === "allowed"
                          ? t("discover_commercial_allowed")
                          : selectedRow.build.commercial_use === "restricted"
                            ? t("discover_commercial_restricted")
                            : t("common_unknown"),
                      ],
                    ].map(([label, value]) => (
                      <div key={label} className="kv-row">
                        <span className="kv-row-label">{label}</span>
                        <span className="kv-row-value mono">{value}</span>
                      </div>
                    ))}
                  </dl>

                  {selectedRow.evaluation && (
                    <div style={{ marginTop: 12 }}>
                      <span className={`badge badge-${fitStatusTone(selectedRow.evaluation.fit.state)}`}>
                        {t(statusLabelKey(selectedRow.evaluation.fit.state))}
                      </span>
                      {selectedRow.evaluation.fit.reasons.map((r) => (
                        <p key={r} className="text-tertiary" style={{ marginTop: 6, fontSize: 11 }}>
                          {r}
                        </p>
                      ))}
                    </div>
                  )}

                  <p className="text-tertiary" style={{ marginTop: 12, fontSize: 11 }}>
                    {t("discover_license_reminder")}
                  </p>

                  <div className="modal-actions">
                    <button
                      className="btn btn-primary"
                      type="button"
                      onClick={() => {
                        const result = checkUrlSafety(selectedRow.build.official_source_url);
                        setModalView(result.safe ? "confirm-open" : "rejected");
                      }}
                    >
                      {t("discover_open_source")}
                    </button>
                    <button className="btn" type="button" onClick={closeModal}>
                      {t("common_close")}
                    </button>
                  </div>
                </>
              )}

              {modalView === "confirm-open" && (
                <>
                  <p className="text-secondary" style={{ marginBottom: 12 }}>
                    {t("discover_confirm_open_body")}
                  </p>
                  <div className="kv-row">
                    <span className="kv-row-label">{t("discover_confirm_open_destination")}</span>
                    <span className="kv-row-value mono">{checkUrlSafety(selectedRow.build.official_source_url).hostname}</span>
                  </div>
                  <p className="text-tertiary" style={{ marginTop: 6, fontSize: 10.5, wordBreak: "break-all" }}>
                    {selectedRow.build.official_source_url}
                  </p>

                  <div className="modal-actions">
                    <button
                      className="btn btn-primary"
                      type="button"
                      onClick={() => {
                        void openUrl(selectedRow.build.official_source_url);
                        closeModal();
                      }}
                    >
                      {t("discover_confirm_open_confirm")}
                    </button>
                    <button className="btn" type="button" onClick={() => setModalView("details")}>
                      {t("common_back")}
                    </button>
                    <button className="btn" type="button" onClick={closeModal}>
                      {t("common_cancel")}
                    </button>
                  </div>
                </>
              )}

              {modalView === "rejected" && (
                <>
                  <p className="text-secondary" style={{ marginBottom: 4 }}>
                    <strong>{t("discover_url_rejected_title")}</strong>
                  </p>
                  <p className="text-tertiary" style={{ marginBottom: 12 }}>
                    {t("discover_url_rejected_body")}
                  </p>

                  <div className="modal-actions">
                    <button className="btn" type="button" onClick={() => setModalView("details")}>
                      {t("common_back")}
                    </button>
                    <button className="btn" type="button" onClick={closeModal}>
                      {t("common_close")}
                    </button>
                  </div>
                </>
              )}
            </Modal>
          )}
        </>
      )}
    </div>
  );
}
