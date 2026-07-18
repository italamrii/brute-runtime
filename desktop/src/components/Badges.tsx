import { useI18n } from "../i18n/I18nContext";
import type { Confidence, Provenance } from "../lib/types";

/** Every hardware/model metric is tagged measured/detected/inferred/
 * unavailable — this badge is the one place that renders that tag, so
 * no page can silently drop it (spec: never a fake universal score). */
export function ConfidenceBadge({ confidence }: { confidence: Confidence | Provenance }) {
  const { t } = useI18n();
  const cls =
    confidence === "measured"
      ? "badge-measured"
      : confidence === "detected"
        ? "badge-detected"
        : confidence === "inferred"
          ? "badge-inferred"
          : confidence === "catalog"
            ? "badge-detected"
            : "badge-unavailable";
  const label =
    confidence === "catalog" ? t("common_catalog") : t(`common_${confidence}`);
  return <span className={`badge ${cls}`}>{label}</span>;
}

