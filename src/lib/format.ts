import type {
  FilterMode,
  SignatureStatus,
  SourceKind,
  StartupItem,
  ViewKind,
} from "@/types";

export function sourceLabel(source: SourceKind): string {
  switch (source) {
    case "registryRun":
      return "注册表";
    case "startupFolder":
      return "启动文件夹";
    case "uwp":
      return "UWP 应用";
    case "scheduledTask":
      return "计划任务";
    case "service":
      return "服务";
    case "contextMenu":
      return "右键菜单";
  }
}

export function sourceShortLabel(source: SourceKind): string {
  switch (source) {
    case "registryRun":
      return "注册表";
    case "startupFolder":
      return "文件夹";
    case "uwp":
      return "UWP";
    case "scheduledTask":
      return "任务";
    case "service":
      return "服务";
    case "contextMenu":
      return "右键菜单";
  }
}

export function scopeLabel(scope: StartupItem["scope"]): string {
  return scope === "allUsers" ? "所有用户" : "当前用户";
}

export function signatureLabel(status: SignatureStatus): string {
  switch (status) {
    case "verified":
      return "签名有效";
    case "invalid":
      return "签名无效";
    case "unknown":
      return "签名未验证";
  }
}

export function filterLabel(filter: FilterMode): string {
  switch (filter) {
    case "all":
      return "全部项目";
    case "enabled":
      return "已启用";
    case "disabled":
      return "已禁用";
    case "thirdParty":
      return "第三方";
    case "unknownPublisher":
      return "未知发布者";
  }
}

export function viewLabel(view: ViewKind): string {
  switch (view) {
    case "login":
      return "登录项";
    case "tasks":
      return "计划任务";
    case "uwp":
      return "UWP 应用";
    case "services":
      return "服务（只读）";
    case "contextMenu":
      return "右键菜单";
    case "snapshots":
      return "快照对比";
  }
}

export function contextMenuLocation(origin: string): string {
  const lower = origin.toLowerCase();
  const isCom = lower.includes("\\shellex\\contextmenuhandlers");
  const multiple = origin.includes("；");
  if (multiple) {
    return isCom ? "多个位置 · COM 扩展" : "多个位置";
  }
  if (lower.includes("\\directory\\background\\")) {
    return isCom ? "文件夹空白处 · COM 扩展" : "文件夹空白处";
  }
  if (lower.includes("\\desktopbackground\\")) {
    return isCom ? "桌面背景 · COM 扩展" : "桌面背景";
  }
  if (lower.includes("\\systemfileassociations\\")) {
    const match = origin.match(/SystemFileAssociations\\([^\\]+)/i);
    const type = match?.[1] ?? "文件类型";
    return isCom ? `${type} · COM 扩展` : type;
  }
  if (lower.includes("\\allfilesystemobjects\\")) {
    return isCom ? "所有文件/文件夹 · COM 扩展" : "所有文件/文件夹";
  }
  if (lower.includes("\\directory\\")) {
    return isCom ? "文件夹 · COM 扩展" : "文件夹";
  }
  if (lower.includes("\\folder\\")) {
    return isCom ? "文件夹（Shell） · COM 扩展" : "文件夹（Shell）";
  }
  if (lower.includes("\\drive\\")) {
    return isCom ? "驱动器 · COM 扩展" : "驱动器";
  }
  if (lower.includes("\\*\\")) {
    return isCom ? "所有文件 · COM 扩展" : "所有文件";
  }
  return isCom ? "COM 扩展" : "右键菜单";
}

export function isMicrosoftItem(item: StartupItem): boolean {
  const origin = item.origin.toLowerCase();
  const publisher = (item.publisher ?? "").toLowerCase();
  return origin.startsWith("\\microsoft\\") || publisher.includes("microsoft");
}

/**
 * Distinguishes a COM shell extension (`shellex\ContextMenuHandlers`) from a
 * classic verb that delegates to a COM handler. Both expose a `COM ` command,
 * but only the former is blocked machine-wide by CLSID.
 */
export function isContextMenuHandler(item: StartupItem): boolean {
  return item.origin
    .toLowerCase()
    .includes("\\shellex\\contextmenuhandlers\\");
}

export function displayPath(value: string | null): string {
  if (!value) {
    return "—";
  }
  return value.replace(/[\\/]/g, (separator) => `${separator}\u200B`);
}

export function shortCommand(command: string | null, limit = 92): string {
  if (!command) {
    return "—";
  }
  const normalized = command.replace(/\s+/g, " ").trim();
  if (normalized.length <= limit) {
    return normalized;
  }
  return `${normalized.slice(0, limit - 1)}…`;
}

const dateTimeFormatter = new Intl.DateTimeFormat("zh-CN", {
  year: "numeric",
  month: "2-digit",
  day: "2-digit",
  hour: "2-digit",
  minute: "2-digit",
  second: "2-digit",
});

export function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }
  return dateTimeFormatter.format(date);
}
