import { useState } from "react";
import { I18nProvider } from "./i18n/I18nContext";
import { AppStatusProvider } from "./lib/AppStatusContext";
import { Shell } from "./components/Shell";
import { ErrorBoundary } from "./components/ErrorBoundary";
import { Onboarding } from "./pages/Onboarding";
import { Overview } from "./pages/Overview";
import { Hardware } from "./pages/Hardware";
import { Models } from "./pages/Models";
import { Discover } from "./pages/Discover";
import { Optimize } from "./pages/Optimize";
import { Run } from "./pages/Run";
import { Profiles } from "./pages/Profiles";
import { Health } from "./pages/Health";
import { Settings } from "./pages/Settings";

export type Page = "overview" | "hardware" | "models" | "discover" | "optimize" | "run" | "profiles" | "health" | "settings";

const ONBOARDED_KEY = "brute.onboarded";

function AppShellRouter() {
  const [onboarded, setOnboarded] = useState<boolean>(() => window.localStorage.getItem(ONBOARDED_KEY) === "1");
  const [page, setPage] = useState<Page>("overview");

  if (!onboarded) {
    return (
      <Onboarding
        onDone={(targetPage) => {
          window.localStorage.setItem(ONBOARDED_KEY, "1");
          setOnboarded(true);
          if (targetPage) setPage(targetPage);
        }}
      />
    );
  }

  return (
    <Shell active={page} onNavigate={setPage}>
      <ErrorBoundary key={page} onRecover={() => setPage("overview")}>
        {page === "overview" && <Overview onNavigate={setPage} />}
        {page === "hardware" && <Hardware />}
        {page === "models" && <Models />}
        {page === "discover" && <Discover />}
        {page === "optimize" && <Optimize />}
        {page === "run" && <Run />}
        {page === "profiles" && <Profiles />}
        {page === "health" && <Health />}
        {page === "settings" && <Settings />}
      </ErrorBoundary>
    </Shell>
  );
}

export default function App() {
  return (
    <I18nProvider>
      <AppStatusProvider>
        <AppShellRouter />
      </AppStatusProvider>
    </I18nProvider>
  );
}
