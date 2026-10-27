export type SourceKind =
  | "registryRun"
  | "startupFolder"
  | "uwp"
  | "scheduledTask"
  | "service"
  | "contextMenu";

export type Scope = "currentUser" | "allUsers";
export type RegView = "64" | "32";
export type SignatureStatus = "verified" | "invalid" | "unknown";

export interface StartupItem {
  id: string;
  name: string;
  command: string | null;
  exePath: string | null;
  icon?: string | null;
  publisher: string | null;
  signature: SignatureStatus;
  source: SourceKind;
  scope: Scope;
  view: RegView | null;
  enabled: boolean;
  readonly: boolean;
  /** True when a write changes machine-wide state and needs elevation. */
  machineWide: boolean;
  origin: string;
  warnings: string[];
}

export interface ScanResult {
  source: SourceKind;
  items: StartupItem[];
  error: string | null;
}

export interface IconEntry {
  path: string;
  icon: string | null;
}

export interface ScanProgress {
  source: SourceKind;
  completed: number;
  total: number;
}

export interface AppState {
  isElevated: boolean;
  version: string;
  /** Account that owns the elevated token. */
  currentUser: string | null;
  /** Account that launched the process before UAC. */
  originalUser: string | null;
  /** UAC was answered with a different account, so HKCU belongs to that one. */
  userMismatch: boolean;
}

export interface SnapshotSummary {
  file: string;
  createdAt: string;
  count: number;
}

export interface ItemChange {
  id: string;
  beforeName: string;
  name: string;
  beforeEnabled: boolean;
  afterEnabled: boolean;
  beforeCommand: string | null;
  afterCommand: string | null;
}

export interface SnapshotDiff {
  added: StartupItem[];
  removed: StartupItem[];
  changed: ItemChange[];
}

export type ViewKind =
  | "login"
  | "tasks"
  | "uwp"
  | "services"
  | "contextMenu"
  | "snapshots";
export type FilterMode =
  | "all"
  | "enabled"
  | "disabled"
  | "thirdParty"
  | "unknownPublisher";

export interface ConfirmRequest {
  item: StartupItem;
  enabled: boolean;
}
