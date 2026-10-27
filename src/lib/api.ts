import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AppState,
  IconEntry,
  ScanProgress,
  ScanResult,
  SnapshotDiff,
  SnapshotSummary,
  SourceKind,
} from "@/types";

export class ApiError extends Error {
  code: string;
  detail: string | null;

  constructor(code: string, message: string, detail: string | null = null) {
    super(message);
    this.name = "ApiError";
    this.code = code;
    this.detail = detail;
  }
}

function normalizeError(error: unknown): ApiError {
  if (error instanceof ApiError) {
    return error;
  }
  if (error && typeof error === "object") {
    const candidate = error as {
      code?: unknown;
      message?: unknown;
      detail?: unknown;
    };
    if (typeof candidate.message === "string") {
      return new ApiError(
        typeof candidate.code === "string" ? candidate.code : "UNKNOWN",
        candidate.message,
        typeof candidate.detail === "string" ? candidate.detail : null,
      );
    }
  }
  return new ApiError("UNKNOWN", String(error));
}

async function call<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw normalizeError(error);
  }
}

export const api = {
  scanFast: () => call<ScanResult[]>("scan_fast"),
  scanTasks: () => call<ScanResult[]>("scan_tasks"),
  scanServices: () => call<ScanResult[]>("scan_services"),
  scanContextMenu: () => call<ScanResult[]>("scan_context_menu"),
  setItemEnabled: (id: string, source: SourceKind, enabled: boolean) =>
    call<void>("set_item_enabled", { id, source, enabled }),
  saveSnapshot: () => call<SnapshotSummary>("save_snapshot"),
  listSnapshots: () => call<SnapshotSummary[]>("list_snapshots"),
  diffSnapshot: (file: string) =>
    call<SnapshotDiff>("diff_snapshot", { file }),
  deleteSnapshot: (file: string) =>
    call<void>("delete_snapshot", { file }),
  openLocation: (id: string, source: SourceKind) =>
    call<void>("open_location", { id, source }),
  loadIcons: (paths: string[]) =>
    call<IconEntry[]>("load_icons", { paths }),
  getAppState: () => call<AppState>("get_app_state"),
  setWindowTheme: (theme: "light" | "dark" | "auto") =>
    call<void>("set_window_theme", { theme }),
};

export function onScanProgress(
  handler: (progress: ScanProgress) => void,
): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan://progress", (event) =>
    handler(event.payload),
  );
}
