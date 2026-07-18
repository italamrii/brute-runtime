import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { useI18n } from "../i18n/I18nContext";
import { auditLibrary, exportLibrary, listLibrary, unquarantineModel } from "../lib/api";
import type { AuditReport, LibraryEntry } from "../lib/types";

function Section({ title, items }: { title: string; items: string[] }) {
  if (items.length === 0) return null;
  return (
    <div style={{ marginBottom: 12 }}>
      <div className="text-secondary" style={{ marginBottom: 4 }}>
        {title} ({items.length})
      </div>
      <ul style={{ margin: 0, paddingInlineStart: 18 }}>
        {items.map((id) => (
          <li key={id} className="mono text-tertiary">
            {id}
          </li>
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
        <h1 className="page-title">{t("health_title")}</h1>
        <div style={{ display: "flex", gap: 8 }}>
          <button className="btn" onClick={refresh} disabled={busy}>
            {t("health_run_audit")}
          </button>
          <button className="btn" onClick={handleExport} disabled={busy}>
            {t("common_export")} (sanitized)
          </button>
        </div>
      </div>

      {error && <div className="error-banner">{error}</div>}

      {report && (
        <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 16 }}>
          <div className="panel">
            <div className="panel-title">Library integrity</div>
            <Section title="Healthy" items={report.healthy} />
            <Section title="Missing" items={report.missing} />
            <Section title="Modified" items={report.modified} />
            <Section title="Corrupt" items={report.corrupt} />
            <Section title="Unsupported" items={report.unsupported} />
            <Section title="Unknown provenance" items={report.unknown_provenance} />
          </div>

          <div className="panel">
            <div className="panel-title">Associations & privacy</div>
            {report.stale_profiles.length === 0 && report.stale_calibrations.length === 0 && (
              <p className="text-secondary">No stale profile or calibration associations.</p>
            )}
            {report.stale_profiles.map((n) => (
              <p key={n.library_id} className="text-tertiary">
                {n.library_id}: {n.detail}
              </p>
            ))}
            {report.privacy_concerns.length === 0 && <p className="text-secondary">No privacy concerns found.</p>}
            {report.privacy_concerns.map((p, i) => (
              <p key={i} className="text-tertiary">
                {p.library_id} — {p.field}: {p.detail}
              </p>
            ))}
            {report.duplicate_groups.length > 0 && (
              <div style={{ marginTop: 8 }}>
                <div className="text-secondary">Duplicate groups ({report.duplicate_groups.length})</div>
                {report.duplicate_groups.map((g) => (
                  <div key={g.sha256} className="text-tertiary mono">
                    {g.sha256.slice(0, 12)}… — {g.members.length} copies
                  </div>
                ))}
              </div>
            )}
            {report.recommendations.length > 0 && (
              <div style={{ marginTop: 8 }}>
                <div className="text-secondary">Recommendations</div>
                {report.recommendations.map((r, i) => (
                  <p key={i} className="text-tertiary">
                    {r}
                  </p>
                ))}
              </div>
            )}
          </div>

          <div className="panel" style={{ gridColumn: "1 / -1" }}>
            <div className="panel-title">Quarantined models</div>
            {quarantined.length === 0 && <p className="text-secondary">None.</p>}
            {quarantined.map((e) => (
              <div key={e.library_id} style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "6px 0", borderBottom: "1px solid var(--border-subtle)" }}>
                <span>
                  {e.alias ?? e.current_path.split(/[\\/]/).pop()} — <span className="text-tertiary">{e.quarantine?.reason}</span>
                </span>
                <button
                  className="btn"
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
                  Unquarantine (re-verify)
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {!report && !error && <div className="text-tertiary">{t("common_loading")}</div>}
    </div>
  );
}
