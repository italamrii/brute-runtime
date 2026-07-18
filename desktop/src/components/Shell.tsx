import type { ReactNode } from "react";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import type { Page } from "../App";

const NAV_ITEMS: { page: Page; key: string }[] = [
  { page: "overview", key: "nav_overview" },
  { page: "hardware", key: "nav_hardware" },
  { page: "models", key: "nav_models" },
  { page: "optimize", key: "nav_optimize" },
  { page: "run", key: "nav_run" },
  { page: "profiles", key: "nav_profiles" },
  { page: "health", key: "nav_health" },
  { page: "settings", key: "nav_settings" },
];

export function Shell({
  active,
  onNavigate,
  children,
}: {
  active: Page;
  onNavigate: (page: Page) => void;
  children: ReactNode;
}) {
  const { t } = useI18n();
  const status = useAppStatus();

  return (
    <div className="app-shell">
      <nav className="sidebar" aria-label={t("app_title")}>
        <div className="sidebar-brand">
          <div className="sidebar-brand-title">{t("app_title")}</div>
          <div className="sidebar-brand-slogan">{t("slogan")}</div>
        </div>
        <ul className="nav-list">
          {NAV_ITEMS.map((item) => (
            <li key={item.page}>
              <button
                type="button"
                className="nav-item"
                aria-current={active === item.page ? "page" : undefined}
                onClick={() => onNavigate(item.page)}
              >
                {t(item.key)}
              </button>
            </li>
          ))}
        </ul>
      </nav>

      <main className="main-area" role="main">
        {children}
      </main>

      <footer className="statusbar">
        <span className="statusbar-item">
          <span className="statusbar-dot" aria-hidden="true" />
          {t("status_offline")}
        </span>
        <span className="statusbar-item">{t("status_no_network")}</span>
        <span className="statusbar-item">
          {t("status_active_model")}: {status.libraryLabel ?? t("status_none")}
        </span>
        <span className="statusbar-item">
          {t("status_active_profile")}: {status.profileId ?? t("status_none")}
        </span>
        <span className="statusbar-item">
          {t("status_backend")}: {status.backend ?? t("status_none")}
        </span>
      </footer>
    </div>
  );
}
