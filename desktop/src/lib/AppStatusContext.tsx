import { createContext, useCallback, useContext, useEffect, useMemo, useState, type ReactNode } from "react";
import { resolveRuntime } from "./api";
import type { Backend, RuntimeResolution } from "./types";

// In-memory only, cleared on restart — the frontend never persists this
// itself; a runtime profile / library entry is the durable record and
// both already live in the Rust core. This is purely "what is the user
// currently looking at" for the status bar and the Run workspace.
//
// `llamaBinPath` is the *resolved* runtime directory actually in use
// (bundled, or the advanced-settings override, or one discovered on
// PATH) — ordinary users never type a path themselves; see
// `commands::runtime::resolve_runtime` and the Settings page's
// Advanced section, which is the only place `manualRuntimeOverride` is
// ever set.
interface AppStatus {
  libraryId: string | null;
  libraryLabel: string | null;
  profileId: string | null;
  backend: Backend | null;
  libraryPath: string;
  llamaBinPath: string;
  runtimeResolution: RuntimeResolution | null;
  manualRuntimeOverride: string;
  setActiveModel: (id: string | null, label: string | null) => void;
  setActiveProfile: (id: string | null, backend: Backend | null) => void;
  setManualRuntimeOverride: (path: string) => void;
  refreshRuntimeResolution: () => Promise<void>;
}

const AppStatusContext = createContext<AppStatus | null>(null);

const RUNTIME_OVERRIDE_STORAGE_KEY = "brute.manualRuntimeOverride";

export function AppStatusProvider({ children }: { children: ReactNode }) {
  const [libraryId, setLibraryId] = useState<string | null>(null);
  const [libraryLabel, setLibraryLabel] = useState<string | null>(null);
  const [profileId, setProfileId] = useState<string | null>(null);
  const [backend, setBackend] = useState<Backend | null>(null);
  const [manualRuntimeOverride, setManualRuntimeOverrideState] = useState<string>(
    () => window.localStorage.getItem(RUNTIME_OVERRIDE_STORAGE_KEY) ?? "",
  );
  const [runtimeResolution, setRuntimeResolution] = useState<RuntimeResolution | null>(null);

  const refreshRuntimeResolution = useCallback(async () => {
    try {
      const resolution = await resolveRuntime(manualRuntimeOverride || null);
      setRuntimeResolution(resolution);
    } catch {
      setRuntimeResolution({
        source: "not_found",
        binary_dir: null,
        cli_verified: false,
        bench_verified: false,
        detail: "Runtime resolution failed unexpectedly.",
      });
    }
  }, [manualRuntimeOverride]);

  // Resolve automatically on startup and whenever the user changes the
  // advanced override — the whole point is that an ordinary user never
  // has to trigger this manually.
  useEffect(() => {
    refreshRuntimeResolution();
  }, [refreshRuntimeResolution]);

  const value = useMemo<AppStatus>(
    () => ({
      libraryId,
      libraryLabel,
      profileId,
      backend,
      libraryPath: "%LOCALAPPDATA%\\BruteRuntime\\library",
      llamaBinPath: runtimeResolution?.binary_dir ?? "",
      runtimeResolution,
      manualRuntimeOverride,
      setActiveModel: (id, label) => {
        setLibraryId(id);
        setLibraryLabel(label);
      },
      setActiveProfile: (id, b) => {
        setProfileId(id);
        setBackend(b);
      },
      setManualRuntimeOverride: (path: string) => {
        setManualRuntimeOverrideState(path);
        window.localStorage.setItem(RUNTIME_OVERRIDE_STORAGE_KEY, path);
      },
      refreshRuntimeResolution,
    }),
    [libraryId, libraryLabel, profileId, backend, runtimeResolution, manualRuntimeOverride, refreshRuntimeResolution],
  );

  return <AppStatusContext.Provider value={value}>{children}</AppStatusContext.Provider>;
}

export function useAppStatus(): AppStatus {
  const ctx = useContext(AppStatusContext);
  if (!ctx) throw new Error("useAppStatus must be used within AppStatusProvider");
  return ctx;
}
