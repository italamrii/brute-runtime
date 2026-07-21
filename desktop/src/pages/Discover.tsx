import { useEffect, useMemo, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { useI18n } from "../i18n/I18nContext";
import { listLibrary, recommendV2 } from "../lib/api";
import type {
  Backend,
  BuildRecommendationV2,
  CapabilityLevel,
  ConfidenceLevel,
  FitState,
  ModelBuild,
  RecommendationCategory,
  SpeedCategory,
  TaskCategory,
  VerificationStatus,
} from "../lib/types";
import { formatBytes } from "../lib/format";
import { checkUrlSafety } from "../lib/urlSafety";
import { Modal } from "../components/Modal";
import { TechnicalValue } from "../components/TechnicalValue";

const GB = 1_000_000_000;
const COMPARE_LIMIT = 4;

function fitStatusTone(state: FitState | null | undefined): "good" | "warn" | "bad" | "unknown" {
  switch (state) {
    case "excellent":
    case "good":
      return "good";
    case "constrained":
    case "experimental":
      return "warn";
    case "not_recommended":
      return "bad";
    default:
      return "unknown";
  }
}

function statusLabelKey(state: FitState | null | undefined): string {
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

function familyKey(build: ModelBuild): string {
  return build.family_id ?? build.family;
}

function sizeClassOf(paramCount: number): "small" | "medium" | "large" | "xlarge" {
  if (paramCount <= 3_000_000_000) return "small";
  if (paramCount <= 8_000_000_000) return "medium";
  if (paramCount <= 15_000_000_000) return "large";
  return "xlarge";
}

function gbInputToBytesOrNull(v: string): number | null {
  const n = Number(v);
  if (!v.trim() || !Number.isFinite(n) || n <= 0) return null;
  return n * GB;
}

function capabilityKey(level: CapabilityLevel): string {
  return `cap_${level}`;
}

function speedKey(level: SpeedCategory): string {
  return `speed_${level}`;
}

function verificationKey(status: VerificationStatus): string {
  return `verification_${status}`;
}

function categoryKey(category: RecommendationCategory): string {
  return `discover_category_${category}`;
}

function confidenceKey(level: ConfidenceLevel): string {
  return `discover_confidence_${level}`;
}

function isGpuBackend(b: Backend): boolean {
  return b === "cuda" || b === "vulkan";
}

interface FiltersState {
  arabic: boolean;
  coding: boolean;
  generalUse: boolean;
  documents: boolean;
  vision: boolean;
  family: string;
  sizeClass: "any" | "small" | "medium" | "large" | "xlarge";
  quantization: string;
  maxDownloadGb: string;
  maxRamGb: string;
  maxVramGb: string;
  cpu: boolean;
  gpu: boolean;
  verifiedSourceOnly: boolean;
  sourceVerified: boolean;
  artifactVerified: boolean;
  installedOnly: boolean;
  recommendedOnly: boolean;
}

const defaultFilters: FiltersState = {
  arabic: false,
  coding: false,
  generalUse: false,
  documents: false,
  vision: false,
  family: "any",
  sizeClass: "any",
  quantization: "any",
  maxDownloadGb: "",
  maxRamGb: "",
  maxVramGb: "",
  cpu: false,
  gpu: false,
  verifiedSourceOnly: false,
  sourceVerified: false,
  artifactVerified: false,
  installedOnly: false,
  recommendedOnly: false,
};

type SortOption = "best_match" | "best_arabic" | "fastest" | "best_quality" | "smallest_download" | "lowest_memory" | "most_trusted";

function tieBreak(a: BuildRecommendationV2, b: BuildRecommendationV2): number {
  return a.build.catalog_id.localeCompare(b.build.catalog_id);
}

function sortEntries(entries: BuildRecommendationV2[], sort: SortOption): BuildRecommendationV2[] {
  const byScore = (score: (e: BuildRecommendationV2) => number, ascending = false) =>
    [...entries].sort((a, b) => (ascending ? score(a) - score(b) : score(b) - score(a)) || tieBreak(a, b));

  switch (sort) {
    case "best_match":
      return byScore((e) => e.overall_score);
    case "best_arabic":
      return byScore((e) => e.component_scores.arabic);
    case "fastest":
      return byScore((e) => e.component_scores.speed);
    case "best_quality":
      return byScore((e) => e.component_scores.quality);
    case "smallest_download":
      return byScore((e) => e.build.file_size_bytes, true);
    case "lowest_memory":
      return byScore((e) => e.build.min_recommended_ram_bytes, true);
    case "most_trusted":
      return byScore((e) => e.component_scores.trust);
    default:
      return entries;
  }
}

const COMPARE_FIELDS: {
  key: string;
  labelKey: string;
  value: (e: BuildRecommendationV2, t: (k: string) => string) => string;
  technical: boolean;
}[] = [
  { key: "family", labelKey: "discover_compare_field_family", value: (e) => familyKey(e.build), technical: true },
  {
    key: "language",
    labelKey: "discover_compare_field_language",
    value: (e, t) => t(capabilityKey(e.build.arabic_capability)),
    technical: false,
  },
  {
    key: "coding",
    labelKey: "discover_compare_field_coding",
    value: (e, t) => t(capabilityKey(e.build.coding_capability)),
    technical: false,
  },
  {
    key: "reasoning",
    labelKey: "discover_compare_field_reasoning",
    value: (e, t) => t(capabilityKey(e.build.reasoning_capability)),
    technical: false,
  },
  {
    key: "documents",
    labelKey: "discover_compare_field_documents",
    value: (e, t) => (e.build.task_categories.includes("document_analysis") ? t("discover_compare_yes") : t("discover_compare_no")),
    technical: false,
  },
  {
    key: "parameters",
    labelKey: "discover_compare_field_parameters",
    value: (e) => e.build.parameter_count.toLocaleString(),
    technical: true,
  },
  { key: "quantization", labelKey: "discover_compare_field_quantization", value: (e) => e.build.quantization, technical: true },
  { key: "size", labelKey: "discover_compare_field_size", value: (e) => formatBytes(e.build.file_size_bytes), technical: true },
  {
    key: "ram",
    labelKey: "discover_compare_field_ram",
    value: (e) => formatBytes(e.build.min_recommended_ram_bytes),
    technical: true,
  },
  {
    key: "vram",
    labelKey: "discover_compare_field_vram",
    value: (e) => (e.build.min_recommended_vram_bytes != null ? formatBytes(e.build.min_recommended_vram_bytes) : "—"),
    technical: true,
  },
  {
    key: "speed",
    labelKey: "discover_compare_field_speed",
    value: (e, t) => t(speedKey(e.build.speed_category)),
    technical: false,
  },
  {
    key: "context",
    labelKey: "discover_compare_field_context",
    value: (e) => (e.build.context_length != null ? e.build.context_length.toLocaleString() : "—"),
    technical: true,
  },
  {
    key: "license",
    labelKey: "discover_compare_field_license",
    value: (e) => (e.build.license.status === "known" ? e.build.license.identifier : "—"),
    technical: true,
  },
  {
    key: "trust",
    labelKey: "discover_compare_field_trust",
    value: (e) => `${Math.round(e.component_scores.trust * 100)}%`,
    technical: true,
  },
  {
    key: "device_fit",
    labelKey: "discover_compare_field_device_fit",
    value: (e) => `${Math.round(e.component_scores.device_fit * 100)}%`,
    technical: true,
  },
];

export function Discover() {
  const { t } = useI18n();
  const [entries, setEntries] = useState<BuildRecommendationV2[] | null>(null);
  const [installedCatalogIds, setInstalledCatalogIds] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
  const [filters, setFilters] = useState<FiltersState>(defaultFilters);
  const [sort, setSort] = useState<SortOption>("best_match");
  const [selected, setSelected] = useState<string | null>(null);
  const [modalView, setModalView] = useState<"details" | "confirm-open" | "rejected">("details");
  const [compareIds, setCompareIds] = useState<string[]>([]);
  const [showCompare, setShowCompare] = useState(false);

  function closeModal() {
    setSelected(null);
    setModalView("details");
  }

  function openSelected(catalogId: string) {
    setSelected(catalogId);
    setModalView("details");
  }

  useEffect(() => {
    Promise.all([recommendV2(), listLibrary()])
      .then(([set, library]) => {
        setEntries(set.entries);
        const installed = new Set<string>();
        for (const entry of library) {
          if (entry.catalog_match.catalog_id && entry.catalog_match.confidence !== "none") {
            installed.add(entry.catalog_match.catalog_id);
          }
        }
        setInstalledCatalogIds(installed);
      })
      .catch((e) => setError(String(e)));
  }, []);

  const families = useMemo(() => {
    if (!entries) return [];
    const set = new Set<string>();
    for (const e of entries) set.add(familyKey(e.build));
    return Array.from(set).sort();
  }, [entries]);

  const quantizations = useMemo(() => {
    if (!entries) return [];
    const set = new Set<string>();
    for (const e of entries) set.add(e.build.quantization);
    return Array.from(set).sort();
  }, [entries]);

  const rows: BuildRecommendationV2[] = useMemo(() => {
    if (!entries) return [];
    const maxDownload = gbInputToBytesOrNull(filters.maxDownloadGb);
    const maxRam = gbInputToBytesOrNull(filters.maxRamGb);
    const maxVram = gbInputToBytesOrNull(filters.maxVramGb);

    const filtered = entries.filter((e) => {
      const b = e.build;
      if (filters.arabic && !b.task_categories.includes("arabic_chat") && b.arabic_capability === "unknown") return false;
      if (filters.coding && !b.task_categories.includes("coding")) return false;
      if (filters.generalUse && !b.task_categories.includes("general_chat")) return false;
      if (filters.documents && !b.task_categories.includes("document_analysis")) return false;
      if (filters.vision && !b.task_categories.includes("vision")) return false;
      if (filters.family !== "any" && familyKey(b) !== filters.family) return false;
      if (filters.sizeClass !== "any" && sizeClassOf(b.parameter_count) !== filters.sizeClass) return false;
      if (filters.quantization !== "any" && b.quantization !== filters.quantization) return false;
      if (maxDownload !== null && b.file_size_bytes > maxDownload) return false;
      if (maxRam !== null && b.min_recommended_ram_bytes > maxRam) return false;
      if (maxVram !== null && b.min_recommended_vram_bytes !== null && b.min_recommended_vram_bytes > maxVram) return false;
      if (filters.cpu && !b.supported_backends.includes("cpu")) return false;
      if (filters.gpu && !b.supported_backends.some(isGpuBackend)) return false;
      if (filters.verifiedSourceOnly && b.license.status === "unknown") return false;
      if (filters.sourceVerified && b.source_verification === "unknown") return false;
      if (filters.artifactVerified && !b.exact_artifact_url) return false;
      if (filters.installedOnly && !installedCatalogIds.has(b.catalog_id)) return false;
      if (filters.recommendedOnly && e.fit_state !== "good" && e.fit_state !== "excellent") return false;
      return true;
    });

    return sortEntries(filtered, sort);
  }, [entries, filters, sort, installedCatalogIds]);

  const selectedEntry = (entries ?? []).find((e) => e.build.catalog_id === selected) ?? null;
  const compareEntries = compareIds.map((id) => (entries ?? []).find((e) => e.build.catalog_id === id)).filter((e): e is BuildRecommendationV2 => !!e);

  function toggleCompare(catalogId: string, checked: boolean) {
    setCompareIds((prev) => {
      if (checked) {
        if (prev.includes(catalogId) || prev.length >= COMPARE_LIMIT) return prev;
        return [...prev, catalogId];
      }
      return prev.filter((id) => id !== catalogId);
    });
  }

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
      {!entries && !error && <div className="loading-block">{t("common_loading")}</div>}

      {entries && (
        <>
          <div className="filter-bar">
            <div className="field">
              <label htmlFor="discover-sort">{t("discover_sort_label")}</label>
              <select id="discover-sort" value={sort} onChange={(e) => setSort(e.target.value as SortOption)}>
                <option value="best_match">{t("discover_sort_best_match")}</option>
                <option value="best_arabic">{t("discover_sort_best_arabic")}</option>
                <option value="fastest">{t("discover_sort_fastest")}</option>
                <option value="best_quality">{t("discover_sort_best_quality")}</option>
                <option value="smallest_download">{t("discover_sort_smallest")}</option>
                <option value="lowest_memory">{t("discover_sort_lowest_memory")}</option>
                <option value="most_trusted">{t("discover_sort_most_trusted")}</option>
              </select>
            </div>

            <div className="field">
              <label htmlFor="discover-filter-family">{t("discover_filter_family")}</label>
              <select
                id="discover-filter-family"
                dir="ltr"
                value={filters.family}
                onChange={(e) => setFilters((f) => ({ ...f, family: e.target.value }))}
              >
                <option value="any">{t("discover_filter_family_any")}</option>
                {families.map((fam) => (
                  <option key={fam} value={fam} dir="ltr">
                    {fam}
                  </option>
                ))}
              </select>
            </div>

            <div className="field">
              <label htmlFor="discover-filter-size">{t("discover_filter_size_class")}</label>
              <select
                id="discover-filter-size"
                value={filters.sizeClass}
                onChange={(e) => setFilters((f) => ({ ...f, sizeClass: e.target.value as FiltersState["sizeClass"] }))}
              >
                <option value="any">{t("discover_filter_size_any")}</option>
                <option value="small">{t("discover_filter_size_small")}</option>
                <option value="medium">{t("discover_filter_size_medium")}</option>
                <option value="large">{t("discover_filter_size_large")}</option>
                <option value="xlarge">{t("discover_filter_size_xlarge")}</option>
              </select>
            </div>

            <div className="field">
              <label htmlFor="discover-filter-quant">{t("discover_filter_quantization")}</label>
              <select
                id="discover-filter-quant"
                dir="ltr"
                value={filters.quantization}
                onChange={(e) => setFilters((f) => ({ ...f, quantization: e.target.value }))}
              >
                <option value="any">{t("discover_filter_quantization_any")}</option>
                {quantizations.map((q) => (
                  <option key={q} value={q} dir="ltr">
                    {q}
                  </option>
                ))}
              </select>
            </div>

            <div className="field">
              <label htmlFor="discover-filter-max-download">{t("discover_filter_max_download")}</label>
              <input
                id="discover-filter-max-download"
                type="number"
                min={0}
                value={filters.maxDownloadGb}
                onChange={(e) => setFilters((f) => ({ ...f, maxDownloadGb: e.target.value }))}
              />
            </div>

            <div className="field">
              <label htmlFor="discover-filter-max-ram">{t("discover_filter_max_ram")}</label>
              <input
                id="discover-filter-max-ram"
                type="number"
                min={0}
                value={filters.maxRamGb}
                onChange={(e) => setFilters((f) => ({ ...f, maxRamGb: e.target.value }))}
              />
            </div>

            <div className="field">
              <label htmlFor="discover-filter-max-vram">{t("discover_filter_max_vram")}</label>
              <input
                id="discover-filter-max-vram"
                type="number"
                min={0}
                value={filters.maxVramGb}
                onChange={(e) => setFilters((f) => ({ ...f, maxVramGb: e.target.value }))}
              />
            </div>

            <button type="button" className="btn btn-ghost btn-sm" onClick={() => setFilters(defaultFilters)}>
              {t("discover_filter_clear")}
            </button>
          </div>

          <div style={{ display: "flex", gap: 10, flexWrap: "wrap", marginBottom: 16 }}>
            {(
              [
                ["arabic", "discover_filter_arabic"],
                ["coding", "discover_filter_coding"],
                ["generalUse", "discover_filter_general"],
                ["documents", "discover_filter_documents"],
                ["vision", "discover_filter_vision"],
                ["cpu", "discover_filter_cpu"],
                ["gpu", "discover_filter_gpu"],
                ["verifiedSourceOnly", "discover_filter_verified"],
                ["sourceVerified", "discover_filter_source_verified"],
                ["artifactVerified", "discover_filter_artifact_verified"],
                ["installedOnly", "discover_filter_installed_only"],
                ["recommendedOnly", "discover_filter_recommended_only"],
              ] as [keyof FiltersState, string][]
            ).map(([key, labelKey]) => (
              <label className="chip-toggle" key={key}>
                <input
                  type="checkbox"
                  checked={filters[key] as boolean}
                  onChange={(e) => setFilters((f) => ({ ...f, [key]: e.target.checked }))}
                />
                {t(labelKey)}
              </label>
            ))}
          </div>

          {compareIds.length > 0 && (
            <div className="filter-bar" style={{ marginBottom: 16 }}>
              <button type="button" className="btn btn-primary btn-sm" onClick={() => setShowCompare(true)}>
                {t("discover_compare_action")} ({compareIds.length})
              </button>
              <button type="button" className="btn btn-sm" onClick={() => setCompareIds([])}>
                {t("discover_compare_clear")}
              </button>
              <span className="text-tertiary" style={{ fontSize: 11 }}>
                {t("discover_compare_limit")}
              </span>
            </div>
          )}

          {rows.length === 0 && <div className="empty-state">{t("discover_empty")}</div>}

          <div className="metric-row" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(270px, 1fr))" }}>
            {rows.map((entry) => {
              const build = entry.build;
              const isInstalled = installedCatalogIds.has(build.catalog_id);
              const extraCategories = entry.categories.filter(
                (c) => c !== "not_recommended" && c !== "unsupported" && c !== "unknown",
              );
              return (
                <div key={build.catalog_id} className="metric-card is-actionable">
                  <button
                    type="button"
                    onClick={() => openSelected(build.catalog_id)}
                    style={{
                      all: "unset",
                      display: "flex",
                      flexDirection: "column",
                      gap: 6,
                      width: "100%",
                      cursor: "pointer",
                    }}
                  >
                    <div className="metric-card-header">
                      <span className="metric-card-label">{build.publisher}</span>
                      <span className={`badge badge-${fitStatusTone(entry.fit_state)}`}>{t(statusLabelKey(entry.fit_state))}</span>
                    </div>
                    <div className="metric-card-value" style={{ fontSize: 15 }}>
                      {build.display_name}
                    </div>
                    <TechnicalValue as="div" className="metric-card-sub">
                      {build.parameter_count.toLocaleString()} · {build.quantization} · {formatBytes(build.file_size_bytes)}
                    </TechnicalValue>
                    <div className="metric-card-foot">
                      {build.task_categories.map((tc: TaskCategory) => (
                        <span key={tc} className="badge badge-unknown">
                          {t(`task_${tc}`)}
                        </span>
                      ))}
                    </div>
                    {(build.speed_category !== "unknown" || build.general_quality !== "unknown") && (
                      <div className="metric-card-foot">
                        {build.speed_category !== "unknown" && (
                          <span className="badge badge-unknown">{t(speedKey(build.speed_category))}</span>
                        )}
                        {build.general_quality !== "unknown" && (
                          <span className="badge badge-unknown">{t(capabilityKey(build.general_quality))}</span>
                        )}
                      </div>
                    )}
                    {extraCategories.length > 0 && (
                      <div className="metric-card-foot">
                        {extraCategories.map((c) => (
                          <span key={c} className="badge badge-good">
                            {t(categoryKey(c))}
                          </span>
                        ))}
                        {isInstalled && <span className="badge badge-good">{t("discover_installed_badge")}</span>}
                      </div>
                    )}
                    {extraCategories.length === 0 && isInstalled && (
                      <div className="metric-card-foot">
                        <span className="badge badge-good">{t("discover_installed_badge")}</span>
                      </div>
                    )}
                  </button>
                  <label className="chip-toggle" style={{ marginTop: 2 }}>
                    <input
                      type="checkbox"
                      checked={compareIds.includes(build.catalog_id)}
                      disabled={!compareIds.includes(build.catalog_id) && compareIds.length >= COMPARE_LIMIT}
                      onChange={(e) => toggleCompare(build.catalog_id, e.target.checked)}
                    />
                    {t("discover_compare_select")}
                  </label>
                </div>
              );
            })}
          </div>

          {selectedEntry && (
            <Modal
              open={selected !== null}
              titleId="discover-modal-title"
              title={modalView === "confirm-open" ? t("discover_confirm_open_title") : t("discover_details_title")}
              onClose={closeModal}
            >
              {modalView === "details" && (
                <>
                  <h3 style={{ marginBottom: 4 }}>{selectedEntry.build.display_name}</h3>
                  <p className="text-tertiary" style={{ marginBottom: 12 }}>
                    {selectedEntry.build.publisher}
                  </p>
                  <p className="text-secondary" style={{ marginBottom: 12 }}>
                    {selectedEntry.build.short_description}
                  </p>

                  <dl style={{ margin: 0 }}>
                    {(
                      [
                        [t("discover_field_params"), selectedEntry.build.parameter_count.toLocaleString(), true],
                        [t("discover_field_quant"), selectedEntry.build.quantization, true],
                        [t("discover_field_size"), formatBytes(selectedEntry.build.file_size_bytes), true],
                        [t("discover_field_ram"), formatBytes(selectedEntry.build.min_recommended_ram_bytes), true],
                        [
                          t("discover_field_vram"),
                          selectedEntry.build.min_recommended_vram_bytes != null
                            ? formatBytes(selectedEntry.build.min_recommended_vram_bytes)
                            : t("common_unknown"),
                          selectedEntry.build.min_recommended_vram_bytes != null,
                        ],
                        [
                          t("discover_field_license"),
                          selectedEntry.build.license.status === "unknown" ? t("common_unknown") : selectedEntry.build.license.identifier,
                          selectedEntry.build.license.status !== "unknown",
                        ],
                        [
                          t("discover_field_commercial"),
                          selectedEntry.build.commercial_use === "allowed"
                            ? t("discover_commercial_allowed")
                            : selectedEntry.build.commercial_use === "restricted"
                              ? t("discover_commercial_restricted")
                              : t("common_unknown"),
                          // Always a translated phrase, never a raw technical
                          // token - never forced ltr.
                          false,
                        ],
                        [t("discover_field_source_verification"), t(verificationKey(selectedEntry.build.source_verification)), false],
                        [t("discover_field_artifact_verification"), t(verificationKey(selectedEntry.build.artifact_verification)), false],
                        [
                          t("discover_field_checksum"),
                          selectedEntry.build.checksum_value ? t("discover_field_checksum_present") : t("discover_field_checksum_absent"),
                          false,
                        ],
                        [t("discover_confidence_label"), t(confidenceKey(selectedEntry.confidence)), false],
                      ] as [string, string, boolean][]
                    ).map(([label, value, technical]) => (
                      <div key={label} className="kv-row">
                        <span className="kv-row-label">{label}</span>
                        {technical ? (
                          <TechnicalValue className="kv-row-value mono">{value}</TechnicalValue>
                        ) : (
                          <span className="kv-row-value mono">{value}</span>
                        )}
                      </div>
                    ))}
                  </dl>

                  {selectedEntry.build.exact_artifact_url && (
                    <p className="text-tertiary" style={{ marginTop: 12, fontSize: 11 }}>
                      {t("discover_download_available_note")}
                    </p>
                  )}

                  <div style={{ marginTop: 12 }}>
                    <span className={`badge badge-${fitStatusTone(selectedEntry.fit_state)}`}>
                      {t(statusLabelKey(selectedEntry.fit_state))}
                    </span>
                  </div>

                  <p className="text-secondary" style={{ marginTop: 12, fontSize: 12 }}>
                    <strong>{t("discover_why_recommended")}: </strong>
                    {selectedEntry.explanation}
                  </p>

                  {selectedEntry.rejection_reasons.length > 0 && (
                    <div style={{ marginTop: 8 }}>
                      <strong style={{ fontSize: 12 }}>{t("discover_why_not_title")}</strong>
                      {selectedEntry.rejection_reasons.map((r) => (
                        <p key={r} className="text-tertiary" style={{ marginTop: 4, fontSize: 11 }}>
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
                        const result = checkUrlSafety(selectedEntry.build.official_source_url);
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
                    <TechnicalValue className="kv-row-value mono">
                      {checkUrlSafety(selectedEntry.build.official_source_url).hostname}
                    </TechnicalValue>
                  </div>
                  <TechnicalValue as="p" className="text-tertiary" style={{ marginTop: 6, fontSize: 10.5, wordBreak: "break-all" }}>
                    {selectedEntry.build.official_source_url}
                  </TechnicalValue>

                  <div className="modal-actions">
                    <button
                      className="btn btn-primary"
                      type="button"
                      onClick={() => {
                        void openUrl(selectedEntry.build.official_source_url);
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

          {showCompare && (
            <Modal
              open={showCompare}
              titleId="discover-compare-title"
              title={t("discover_compare_title")}
              onClose={() => setShowCompare(false)}
            >
              {compareEntries.length === 0 ? (
                <p className="text-tertiary">{t("discover_compare_empty")}</p>
              ) : (
                <div className="table-scroll">
                  <table className="data-table">
                    <thead>
                      <tr>
                        <th></th>
                        {compareEntries.map((e) => (
                          <th key={e.build.catalog_id}>{e.build.display_name}</th>
                        ))}
                      </tr>
                    </thead>
                    <tbody>
                      {COMPARE_FIELDS.map((field) => (
                        <tr key={field.key}>
                          <td>{t(field.labelKey)}</td>
                          {compareEntries.map((e) =>
                            field.technical ? (
                              <td key={e.build.catalog_id}>
                                <TechnicalValue>{field.value(e, t)}</TechnicalValue>
                              </td>
                            ) : (
                              <td key={e.build.catalog_id}>{field.value(e, t)}</td>
                            ),
                          )}
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
              <div className="modal-actions">
                <button type="button" className="btn" onClick={() => setCompareIds([])}>
                  {t("discover_compare_clear")}
                </button>
                <button type="button" className="btn" onClick={() => setShowCompare(false)}>
                  {t("common_close")}
                </button>
              </div>
            </Modal>
          )}
        </>
      )}
    </div>
  );
}
