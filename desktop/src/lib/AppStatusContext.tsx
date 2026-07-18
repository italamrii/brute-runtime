import { createContext, useContext, useMemo, useState, type ReactNode } from "react";
import type { Backend } from "./types";

// In-memory only, cleared on restart — the frontend never persists this
// itself; a runtime profile / library entry is the durable record and
// both already live in the Rust core. This is purely "what is the user
// currently looking at" for the status bar and the Run workspace.
interface AppStatus {
  libraryId: string | null;
  libraryLabel: string | null;
  profileId: string | null;
  backend: Backend | null;
  libraryPath: string;
  llamaBinPath: string;
  setActiveModel: (id: string | null, label: string | null) => void;
  setActiveProfile: (id: string | null, backend: Backend | null) => void;
  setLlamaBinPath: (path: string) => void;
}

const AppStatusContext = createContext<AppStatus | null>(null);

const LLAMA_BIN_STORAGE_KEY = "brute.llamaBinPath";

export function AppStatusProvider({ children }: { children: ReactNode }) {
  const [libraryId, setLibraryId] = useState<string | null>(null);
  const [libraryLabel, setLibraryLabel] = useState<string | null>(null);
  const [profileId, setProfileId] = useState<string | null>(null);
  const [backend, setBackend] = useState<Backend | null>(null);
  const [llamaBinPath, setLlamaBinPathState] = useState<string>(
    () => window.localStorage.getItem(LLAMA_BIN_STORAGE_KEY) ?? "",
  );

  const value = useMemo<AppStatus>(
    () => ({
      libraryId,
      libraryLabel,
      profileId,
      backend,
      libraryPath: "%LOCALAPPDATA%\\BruteRuntime\\library",
      llamaBinPath,
      setActiveModel: (id, label) => {
        setLibraryId(id);
        setLibraryLabel(label);
      },
      setActiveProfile: (id, b) => {
        setProfileId(id);
        setBackend(b);
      },
      setLlamaBinPath: (path: string) => {
        setLlamaBinPathState(path);
        window.localStorage.setItem(LLAMA_BIN_STORAGE_KEY, path);
      },
    }),
    [libraryId, libraryLabel, profileId, backend, llamaBinPath],
  );

  return <AppStatusContext.Provider value={value}>{children}</AppStatusContext.Provider>;
}

export function useAppStatus(): AppStatus {
  const ctx = useContext(AppStatusContext);
  if (!ctx) throw new Error("useAppStatus must be used within AppStatusProvider");
  return ctx;
}
