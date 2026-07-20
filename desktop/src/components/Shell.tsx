import type { ReactNode } from "react";
import { useI18n } from "../i18n/I18nContext";
import { useAppStatus } from "../lib/AppStatusContext";
import { brand } from "../config/brand";
import type { Page } from "../App";
import {
  IconChat,
  IconDiscover,
  IconHardware,
  IconHealth,
  IconModels,
  IconOptimize,
  IconOverview,
  IconProfiles,
  IconRun,
  IconSettings,
} from "./Icons";

const NAV_ITEMS: { page: Page; key: string; Icon: typeof IconOverview }[] = [
  { page: "chat", key: "nav_chat", Icon: IconChat },
  { page: "overview", key: "nav_overview", Icon: IconOverview },
  { page: "hardware", key: "nav_hardware", Icon: IconHardware },
  { page: "models", key: "nav_models", Icon: IconModels },
  { page: "discover", key: "nav_discover", Icon: IconDiscover },
  { page: "optimize", key: "nav_optimize", Icon: IconOptimize },
  { page: "run", key: "nav_run", Icon: IconRun },
  { page: "profiles", key: "nav_profiles", Icon: IconProfiles },
  { page: "health", key: "nav_health", Icon: IconHealth },
  { page: "settings", key: "nav_settings", Icon: IconSettings },
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
          <div className="sidebar-brand-mark">
            <img className="sidebar-brand-glyph" src={brand.iconUrl} alt="" aria-hidden="true" />
            <div>
              <div className="sidebar-brand-title">{brand.shortName}</div>
              <div className="sidebar-brand-sub">Runtime</div>
            </div>
          </div>
          <div className="sidebar-brand-slogan">{t("slogan")}</div>
        </div>

        <div className="nav-section-label">{t("nav_section_console")}</div>
        <ul className="nav-list">
          {NAV_ITEMS.map((item) => (
            <li key={item.page}>
              <button
                type="button"
                className="nav-item"
                aria-current={active === item.page ? "page" : undefined}
                onClick={() => onNavigate(item.page)}
              >
                <span className="nav-item-icon">
                  <item.Icon />
                </span>
                {t(item.key)}
              </button>
            </li>
          ))}
        </ul>

        <div className="sidebar-footer">
          <div className="sidebar-footer-row">
            <span className="sidebar-footer-dot" aria-hidden="true" />
            <span>{t("status_offline")}</span>
          </div>
          <div className="sidebar-footer-row" style={{ marginTop: 6 }}>
            <span className="text-tertiary">v0.1.0</span>
          </div>
        </div>
      </nav>

      <main className="main-area" role="main">
        {children}
      </main>

      <footer className="statusbar" aria-label={t("status_bar_label")}>
        <span className="statusbar-item">
          <span className="statusbar-dot" aria-hidden="true" />
          <strong>{t("status_offline")}</strong>
        </span>
        <span className="statusbar-sep" aria-hidden="true" />
        <span className="statusbar-item">{t("status_no_network")}</span>
        <span className="statusbar-sep" aria-hidden="true" />
        <span className="statusbar-item">
          {t("status_active_model")}: <strong>{status.libraryLabel ?? t("status_none")}</strong>
        </span>
        <span className="statusbar-sep" aria-hidden="true" />
        <span className="statusbar-item">
          {t("status_active_profile")}: <strong>{status.profileId ?? t("status_none")}</strong>
        </span>
        <span className="statusbar-sep" aria-hidden="true" />
        <span className="statusbar-item">
          {t("status_backend")}: <strong>{status.backend ?? t("status_none")}</strong>
        </span>
      </footer>
    </div>
  );
}
