// Thin typed wrappers around every Tauri command exposed by
// desktop/src-tauri/src/commands/. This is the *only* place the
// frontend calls `invoke` — every page imports from here, never calls
// `invoke` directly, so the full command surface stays auditable in one
// file. See docs/tauri-security-boundary.md.

import { invoke } from "@tauri-apps/api/core";
import type {
  ApplyResult,
  AssociationsDto,
  AuditReport,
  Backend,
  BackendVerification,
  CalibrationStore,
  DuplicateGroup,
  ExplainFitDto,
  HardwareCapabilityProfile,
  ImportDirectoryOutcome,
  ImportOutcome,
  LibraryEntry,
  LocalRunOutcome,
  LocateOutcome,
  ModelBuild,
  Priority,
  RankingPriority,
  RecommendationDto,
  RefreshOutcomeDto,
  RuntimeProfile,
  ScanOptionsDto,
  ScanResult,
  StorageSummary,
  BuildEvaluation,
  TaskCategory,
  TunePlanDto,
  TuneRunDto,
  VerifyOutcomeDto,
} from "./types";

export class BruteApiError extends Error {}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (e) {
    throw new BruteApiError(typeof e === "string" ? e : String(e));
  }
}

// Hardware -------------------------------------------------------------
export const getHardwareProfile = () => call<HardwareCapabilityProfile>("hardware_profile");

// Catalog / fit / recommendation ---------------------------------------
export const listCatalog = () => call<ModelBuild[]>("catalog_list");
export const showCatalogEntry = (catalog_id: string) => call<ModelBuild>("catalog_show", { catalog_id });
export const listCalibrations = () => call<CalibrationStore>("calibrations_list");
export const evaluateFit = (catalog_id: string, context?: number, backend?: Backend) =>
  call<BuildEvaluation>("fit_evaluate", { catalog_id, context: context ?? null, backend: backend ?? null });
export const recommendModel = (task: TaskCategory | null, priority: Priority) =>
  call<RecommendationDto>("recommend_model", { task, priority });
export const explainFit = (catalog_id: string, task: TaskCategory | null, priority: Priority) =>
  call<ExplainFitDto>("explain_fit", { catalog_id, task, priority });

// Backends ---------------------------------------------------------------
export const verifyBackends = (
  model: string,
  llama_bin: string,
  backend: Backend | null,
  allow_unverified_binary: boolean,
  timeout_secs: number,
) => call<BackendVerification[]>("backends_verify", { model, llama_bin, backend, allow_unverified_binary, timeout_secs });

// Trusted local model library --------------------------------------------
export const listLibrary = () => call<LibraryEntry[]>("library_list");
export const showLibraryEntry = (library_id: string) => call<LibraryEntry>("library_show", { library_id });
export const getAssociations = (library_id: string) => call<AssociationsDto>("library_associations", { library_id });
export const importModel = (path: string, alias: string | null) =>
  call<ImportOutcome>("library_import", { path, alias });
export const scanDirectory = (root: string, options: ScanOptionsDto) =>
  call<ScanResult>("library_scan", { root, options });
export const importDirectory = (root: string, options: ScanOptionsDto) =>
  call<ImportDirectoryOutcome>("library_import_directory", { root, options });
export const verifyLibraryEntries = (library_id: string | null, all: boolean) =>
  call<VerifyOutcomeDto[]>("library_verify", { library_id, all });
export const refreshLibraryEntries = (library_id: string | null, all: boolean) =>
  call<RefreshOutcomeDto[]>("library_refresh", { library_id, all });
export const auditLibrary = () => call<AuditReport>("library_audit");
export const listDuplicates = () => call<DuplicateGroup[]>("library_duplicates");
export const getStorageSummary = () => call<StorageSummary>("library_storage");
export const locateModel = (library_id: string, new_path: string) =>
  call<LocateOutcome>("library_locate", { library_id, new_path });
export const setAlias = (library_id: string, name: string) => call<void>("library_alias", { library_id, name });
export const setNote = (library_id: string, text: string) => call<void>("library_note", { library_id, text });
export const forgetModel = (library_id: string) => call<void>("library_forget", { library_id });
export const quarantineModel = (library_id: string, reason: string) =>
  call<void>("library_quarantine", { library_id, reason });
export const unquarantineModel = (library_id: string) =>
  call<import("./types").GgufVerification>("library_unquarantine", { library_id });
export const listQuarantined = () => call<LibraryEntry[]>("library_quarantined");
export const exportLibrary = (output_path: string) => call<number>("library_export", { output_path });

// Auto-tuning ------------------------------------------------------------
export const tuneDryRun = (model: string, llama_bin: string, backend: Backend | null, allow_unverified_binary: boolean) =>
  call<TunePlanDto>("tune_dry_run", { model, llama_bin, backend, allow_unverified_binary });

export const tuneRun = (args: {
  model: string;
  llama_bin: string;
  priority: RankingPriority;
  backend: Backend | null;
  max_duration_secs: number | null;
  allow_unverified_binary: boolean;
  save_profile: boolean;
}) => call<TuneRunDto>("tune_run", args);

export const tuneCancel = () => call<void>("tune_cancel");

// Runtime profiles ---------------------------------------------------------
export const listProfiles = () => call<string[]>("profiles_list");
export const showProfile = (profile_id: string) => call<RuntimeProfile>("profiles_show", { profile_id });
export const verifyProfile = (
  profile_id: string,
  model: string,
  llama_bin: string,
  allow_unverified_binary: boolean,
  timeout_secs: number,
) => call<ApplyResult>("profiles_verify", { profile_id, model, llama_bin, allow_unverified_binary, timeout_secs });
export const exportProfile = (profile_id: string, output_path: string) =>
  call<void>("profiles_export", { profile_id, output_path });
export const deleteProfile = (profile_id: string) => call<void>("profiles_delete", { profile_id });

// Local run workspace --------------------------------------------------
export const localRunGenerate = (args: {
  library_id: string;
  profile_id: string;
  llama_bin: string;
  prompt: string;
  allow_unverified_binary: boolean;
}) => call<LocalRunOutcome>("local_run_generate", args);
export const localRunCancel = () => call<void>("local_run_cancel");
