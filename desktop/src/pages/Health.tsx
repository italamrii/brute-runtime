import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { auditLibrary, exportLibrary, listLibrary, unquarantineModel } from "../lib/api";
import type { AuditReport, LibraryEntry } from "../lib/types";
import { TechnicalValue } from "../components/TechnicalValue";

function IssueBlock({
  title,
  items,
  tone,
  why,
  next,
}: {
  title: string;
  items: string[];
  tone: "good" | "warn" | "bad" | "unknown";
  why: string;
  next: string;
}) {
  if (items.length === 0) return null;
  return (
    <div className="issue-block">
      <div className="issue-title">
          <span className={`status-dot${tone === "good" ? "" : ` is-${tone}`}`} aria-hidden="true" />
        {title} ({items.length})
      </div>
      <p className="text-secondary" style={{ fontSize: 11.5, marginBottom: 4 }}>
        {why}
      </p>
      <p className="text-tertiary" style={{ fontSize: 11, marginBottom: 6 }}>
        {next}
      </p>
      <ul className="issue-list">
        {items.map((id) => (
          <TechnicalValue as="li" key={id}>
            {id}
          </TechnicalValue>
        ))}
      </ul>
    </div>
  );
}

export function Health() {
  const { t } = useI18n();
  const [report, setReport] = useState<AuditReport | null>(null);
  const [entries, setEntries] = useState<LibraryEntry[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  function refresh() {
    setBusy(true);
    Promise.all([auditLibrary(), listLibrary()])
      .then(([a, e]) => {
        setReport(a);
        setEntries(e);
      })
      .catch((e) => setError(String(e)))
      .finally(() => setBusy(false));
  }

  useEffect(refresh, []);

  const quarantined = entries.filter((e) => e.quarantine !== null);

  async function handleExport() {
    const target = await save({ defaultPath: "brute-library-export.json" });
    if (!target) return;
    setBusy(true);
    try {
      await exportLibrary(target);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="page">
      <div className="page-header">
        <div>
          <div className="page-kicker">{t("health_kicker")}</div>
          <h1 className="page-title">{t("health_title")}</h1>
          <p className="page-desc">{t("health_desc")}</p>
        </div>
        <div className="page-actions">
          <button className="btn" type="button" onClick={refresh} disabled={busy}>
            {t("health_run_audit")}
          </button>
          <button className="btn" type="button" onClick={handleExport} disabled={busy}>
            {t("common_export")}
          </button>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}
      {!report && !error && <div className="loading-block">{t("common_loading")}</div>}

      {report && (
        <>
          <div className="summary-strip">
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("overview_models_healthy")}</div>
              <div className="summary-strip-value num">{report.healthy.length}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("health_missing")}</div>
              <div className="summary-strip-value num">{report.missing.length}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("health_changed")}</div>
              <div className="summary-strip-value num">{report.modified.length}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("health_corrupt")}</div>
              <div className="summary-strip-value num">{report.corrupt.length}</div>
            </div>
            <div className="summary-strip-item">
              <div className="summary-strip-label">{t("models_quarantined_badge")}</div>
              <div className="summary-strip-value num">{quarantined.length}</div>
            </div>
          </div>

          <div className="health-grid">
            <div className="panel">
              <div className="panel-title">{t("health_integrity")}</div>
              <IssueBlock
                title={t("overview_models_healthy")}
                items={report.healthy}
                tone="good"
                why={t("health_why_healthy")}
                next={t("health_next_healthy")}
              />
              <IssueBlock
                title={t("health_missing")}
                items={report.missing}
                tone="bad"
                why={t("health_why_missing")}
                next={t("health_next_missing")}
              />
              <IssueBlock
                title={t("health_changed")}
                items={report.modified}
                tone="warn"
                why={t("health_why_changed")}
                next={t("health_next_changed")}
              />
              <IssueBlock
                title={t("health_corrupt")}
                items={report.corrupt}
                tone="bad"
                why={t("health_why_corrupt")}
                next={t("health_next_corrupt")}
              />
              <IssueBlock
                title={t("health_unsupported")}
                items={report.unsupported}
                tone="warn"
                why={t("health_why_unsupported")}
                next={t("health_next_unsupported")}
              />
              <IssueBlock
                title={t("health_unknown_prov")}
                items={report.unknown_provenance}
                tone="unknown"
                why={t("health_why_unknown")}
                next={t("health_next_unknown")}
              />
            </div>

            <div className="panel">
              <div className="panel-title">{t("health_associations")}</div>
              {report.stale_profiles.length === 0 && report.stale_calibrations.length === 0 && (
                <p className="text-secondary">{t("health_no_stale")}</p>
              )}
              {report.stale_profiles.map((n) => (
                <p key={n.library_id} className="text-tertiary" style={{ marginBottom: 6 }}>
                  <TechnicalValue as="span">{n.library_id}</TechnicalValue>: {n.detail}
                </p>
              ))}
              {report.stale_calibrations.map((n) => (
                <p key={`cal-${n.library_id}`} className="text-tertiary" style={{ marginBottom: 6 }}>
                  <TechnicalValue as="span">{n.library_id}</TechnicalValue>: {n.detail}
                </p>
              ))}

              {report.privacy_concerns.length === 0 ? (
                <p className="text-secondary" style={{ marginTop: 12 }}>
                  {t("health_no_privacy")}
                </p>
              ) : (
                report.privacy_concerns.map((p, i) => (
                  <p key={i} className="text-tertiary">
                    <TechnicalValue as="span">{p.library_id}</TechnicalValue> — {p.field}: {p.detail}
                  </p>
                ))
              )}

              {report.duplicate_groups.length > 0 && (
                <div style={{ marginTop: 12 }}>
                  <div className="issue-title">
                    {t("health_duplicates")} ({report.duplicate_groups.length})
                  </div>
                  {report.duplicate_groups.map((g) => (
                    <div key={g.sha256} className="text-tertiary mono">
                      <TechnicalValue as="span" title={g.sha256}>
                        {g.sha256.slice(0, 12)}…
                      </TechnicalValue>{" "}
                      — {g.members.length} {t("health_copies")}
                    </div>
                  ))}
                </div>
              )}

              {report.recommendations.length > 0 && (
                <div style={{ marginTop: 12 }}>
                  <div className="issue-title">{t("health_recommendations")}</div>
                  {report.recommendations.map((r, i) => (
                    <p key={i} className="text-tertiary">
                      {r}
                    </p>
                  ))}
                </div>
              )}
            </div>

            <div className="panel" style={{ gridColumn: "1 / -1" }}>
              <div className="panel-title">{t("models_quarantined_badge")}</div>
              {quarantined.length === 0 && <p className="text-secondary">{t("common_none")}</p>}
              {quarantined.map((e) => (
                <div key={e.library_id} className="kv-row">
                  <span>
                    <TechnicalValue as="span">{e.alias ?? e.current_path.split(/[\\/]/).pop()}</TechnicalValue> —{" "}
                    <span className="text-tertiary">{e.quarantine?.reason}</span>
                  </span>
                  <button
                    className="btn btn-sm"
                    type="button"
                    disabled={busy}
                    onClick={async () => {
                      setBusy(true);
                      try {
                        await unquarantineModel(e.library_id);
                        refresh();
                      } catch (err) {
                        setError(String(err));
                      } finally {
                        setBusy(false);
                      }
                    }}
                  >
                    {t("models_unquarantine")}
                  </button>
                </div>
              ))}
            </div>
          </div>
        </>
      )}
    </div>
  );
}
