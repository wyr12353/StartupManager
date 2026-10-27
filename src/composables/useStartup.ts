import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { toast } from "vue-sonner";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api, ApiError, onScanProgress } from "@/lib/api";
import { isMicrosoftItem } from "@/lib/format";
import type {
  AppState,
  FilterMode,
  ScanProgress,
  ScanResult,
  SnapshotDiff,
  SnapshotSummary,
  SourceKind,
  StartupItem,
  ViewKind,
} from "@/types";

const nameCollator = new Intl.Collator("zh-CN");
const ICON_BATCH_SIZE = 50;
const ICON_CACHE_LIMIT = 512;
const ICON_RETRY_MS = 60_000;
/// Re-scan the right-click menu when the cached list is older than this.
const CONTEXT_MENU_STALE_MS = 15_000;

const sourceByView: Record<Exclude<ViewKind, "snapshots">, SourceKind[]> = {
  login: ["registryRun", "startupFolder"],
  tasks: ["scheduledTask"],
  uwp: ["uwp"],
  services: ["service"],
  contextMenu: ["contextMenu"],
};

function errorMessage(error: unknown): string {
  if (error instanceof ApiError) {
    return error.detail ? `${error.message}：${error.detail}` : error.message;
  }
  return String(error);
}

export function useStartup() {
  const items = ref<StartupItem[]>([]);
  const loading = ref(true);
  const scanning = ref(false);
  const servicesLoading = ref(false);
  const contextMenuLoading = ref(false);
  const contextMenuLoaded = ref(false);
  const contextMenuLoadedAt = ref(0);
  const errors = ref<Partial<Record<SourceKind, string>>>({});
  const fatalError = ref<string | null>(null);
  const progress = ref<ScanProgress | null>(null);
  const appState = ref<AppState | null>(null);

  const view = ref<ViewKind>("login");
  const filter = ref<FilterMode>("all");
  const query = ref("");
  const selectedId = ref<string | null>(null);
  const busyIds = ref<Set<string>>(new Set());

  const snapshots = ref<SnapshotSummary[]>([]);
  const snapshotsLoading = ref(true);
  const snapshotsError = ref<string | null>(null);
  const snapshotSaving = ref(false);
  const deletingSnapshot = ref<string | null>(null);
  const activeSnapshot = ref<string | null>(null);
  const diff = ref<SnapshotDiff | null>(null);
  const diffLoading = ref(false);

  const iconMap = new Map<string, string>();
  const iconInFlight = new Set<string>();
  const iconFailedAt = new Map<string, number>();
  let iconLoading = false;
  let iconPending = false;
  let iconTimer: number | undefined;

  let unlisten: UnlistenFn | null = null;
  let disposed = false;
  let scanRun = 0;
  let scanInFlight: Promise<void> | null = null;
  let diffRun = 0;
  let snapshotRun = 0;
  let contextMenuInFlight: Promise<void> | null = null;
  let contextMenuRun = 0;

  function iconKey(item: StartupItem): string | null {
    return item.exePath ? item.exePath.toLowerCase() : null;
  }

  function attachIcon(item: StartupItem): StartupItem {
    const key = iconKey(item);
    if (!key) {
      return item;
    }
    const icon = iconMap.get(key);
    return icon ? { ...item, icon } : item;
  }

  function scheduleIcons() {
    if (disposed) {
      return;
    }
    if (iconTimer !== undefined) {
      window.clearTimeout(iconTimer);
    }
    iconTimer = window.setTimeout(() => {
      iconTimer = undefined;
      void loadIcons();
    }, 120);
  }

  async function loadIcons() {
    if (disposed) {
      return;
    }
    if (iconLoading) {
      iconPending = true;
      return;
    }
    const paths = new Map<string, string>();
    for (const item of viewItems.value) {
      if (item.exePath) {
        paths.set(item.exePath.toLowerCase(), item.exePath);
      }
    }
    const now = Date.now();
    const missing: string[] = [];
    for (const [key, path] of paths) {
      if (iconMap.has(key) || iconInFlight.has(key)) {
        continue;
      }
      const failedAt = iconFailedAt.get(key);
      if (failedAt === undefined || now - failedAt >= ICON_RETRY_MS) {
        missing.push(path);
      }
    }
    if (missing.length === 0) {
      return;
    }

    iconLoading = true;
    for (const path of missing) {
      iconInFlight.add(path.toLowerCase());
    }
    try {
      for (let start = 0; start < missing.length; start += ICON_BATCH_SIZE) {
        const batch = missing.slice(start, start + ICON_BATCH_SIZE);
        const entries = await api.loadIcons(batch);
        if (disposed) {
          return;
        }
        for (const entry of entries) {
          const key = entry.path.toLowerCase();
          if (entry.icon) {
            iconMap.set(key, entry.icon);
            iconFailedAt.delete(key);
          } else {
            iconFailedAt.set(key, Date.now());
          }
        }
      }
      while (iconMap.size > ICON_CACHE_LIMIT) {
        const oldest = iconMap.keys().next().value;
        if (oldest === undefined) {
          break;
        }
        iconMap.delete(oldest);
      }
      items.value = items.value.map(attachIcon);
    } catch {
      // Retry on the next scan rather than poisoning the cache forever.
    } finally {
      for (const path of missing) {
        iconInFlight.delete(path.toLowerCase());
      }
      iconLoading = false;
      if (iconPending && !disposed) {
        iconPending = false;
        scheduleIcons();
      }
    }
  }

  const viewItems = computed(() => {
    if (view.value === "snapshots") {
      return [];
    }
    const sources = sourceByView[view.value];
    return items.value.filter((item) => sources.includes(item.source));
  });

  const visibleItems = computed(() => {
    let list = viewItems.value;
    const normalizedQuery = query.value.trim().toLowerCase();
    if (normalizedQuery) {
      list = list.filter((item) =>
        [item.name, item.command, item.publisher, item.origin].some((value) =>
          value?.toLowerCase().includes(normalizedQuery),
        ),
      );
    }
    switch (filter.value) {
      case "enabled":
        list = list.filter((item) => item.enabled);
        break;
      case "disabled":
        list = list.filter((item) => !item.enabled);
        break;
      case "thirdParty":
        list = list.filter((item) => !isMicrosoftItem(item));
        break;
      case "unknownPublisher":
        list = list.filter((item) => !item.publisher);
        break;
      case "all":
        break;
    }
    return [...list].sort((a, b) => nameCollator.compare(a.name, b.name));
  });

  const counts = computed<Record<ViewKind, number>>(() => {
    let login = 0;
    let tasks = 0;
    let uwp = 0;
    let services = 0;
    let contextMenu = 0;
    for (const item of items.value) {
      switch (item.source) {
        case "registryRun":
        case "startupFolder":
          login += 1;
          break;
        case "scheduledTask":
          tasks += 1;
          break;
        case "uwp":
          uwp += 1;
          break;
        case "service":
          services += 1;
          break;
        case "contextMenu":
          contextMenu += 1;
          break;
      }
    }
    return {
      login,
      tasks,
      uwp,
      services,
      contextMenu,
      snapshots: snapshots.value.length,
    };
  });

  const statusCounts = computed(() => {
    let enabled = 0;
    for (const item of viewItems.value) {
      if (item.enabled) {
        enabled += 1;
      }
    }
    return { enabled, disabled: viewItems.value.length - enabled };
  });
  const enabledCount = computed(() => statusCounts.value.enabled);
  const disabledCount = computed(() => statusCounts.value.disabled);
  const selectedItem = computed(
    () => items.value.find((item) => item.id === selectedId.value) ?? null,
  );
  const currentLoading = computed(
    () =>
      loading.value ||
      (view.value === "services" && servicesLoading.value) ||
      (view.value === "contextMenu" && contextMenuLoading.value),
  );

  function replaceItems(next: StartupItem[]) {
    items.value = next;
    if (
      selectedId.value &&
      !next.some((item) => item.id === selectedId.value)
    ) {
      selectedId.value = null;
    }
  }

  function updateItem(next: StartupItem) {
    items.value = items.value.map((item) => {
      if (item.id !== next.id) {
        return item;
      }
      const icon =
        item.exePath === next.exePath
          ? (item.icon ?? attachIcon(next).icon)
          : attachIcon(next).icon;
      return { ...next, icon };
    });
  }

  function setSourceError(source: SourceKind, message: string) {
    errors.value = { ...errors.value, [source]: message };
  }

  function clearSourceError(source: SourceKind) {
    const next = { ...errors.value };
    delete next[source];
    errors.value = next;
  }

  function mergeResult(result: ScanResult) {
    if (result.error) {
      setSourceError(result.source, result.error);
    } else {
      clearSourceError(result.source);
    }
    if (result.error && result.items.length === 0) {
      // A fully failed refresh must not make the list look empty; a partial
      // failure still merges whatever was scanned.
      return;
    }
    const others = items.value.filter((item) => item.source !== result.source);
    const next = result.items.map(attachIcon);
    replaceItems([...others, ...next]);
    scheduleIcons();
  }

  function applySnapshotList(list: SnapshotSummary[]) {
    if (!Array.isArray(list)) {
      snapshotsError.value = "快照列表格式异常";
      return;
    }
    snapshots.value = list;
    snapshotsError.value = null;
    if (
      activeSnapshot.value &&
      !list.some((snapshot) => snapshot.file === activeSnapshot.value)
    ) {
      diffRun += 1;
      activeSnapshot.value = null;
      diff.value = null;
      diffLoading.value = false;
    }
  }

  async function runScan(run: number) {
    loading.value = true;
    scanning.value = true;
    snapshotsLoading.value = true;
    snapshotsError.value = null;
    progress.value = null;
    fatalError.value = null;
    errors.value = {};

    // These do not depend on the scan; start them immediately so the sidebar
    // and snapshot page do not show stale/empty state while services load.
    void api.getAppState().then(
      (value) => {
        if (run === scanRun) {
          appState.value = value;
        }
      },
      () => {
        // Keep the previous state; SideNav shows "权限未知" when it is null.
      },
    );
    const snapshotToken = ++snapshotRun;
    void api.listSnapshots().then(
      (value) => {
        if (run === scanRun && snapshotToken === snapshotRun) {
          applySnapshotList(value);
          snapshotsLoading.value = false;
        }
      },
      (reason: unknown) => {
        if (run === scanRun && snapshotToken === snapshotRun) {
          const apiError = reason instanceof ApiError ? reason : null;
          snapshotsError.value = apiError?.detail
            ? `${apiError.message}：${apiError.detail}`
            : (apiError?.message ?? "读取快照失败");
          if (view.value === "snapshots") {
            toast.error(apiError?.message ?? "读取快照失败", {
              description: apiError?.detail ?? undefined,
            });
          }
          snapshotsLoading.value = false;
        }
      },
    );

    try {
      const fast = api
        .scanFast()
        .then((results) => {
          if (run === scanRun) {
            results.forEach(mergeResult);
          }
        })
        .catch((error) => {
          if (run === scanRun) {
            fatalError.value ??= errorMessage(error);
          }
        });
      const tasks = api
        .scanTasks()
        .then((results) => {
          if (run === scanRun) {
            results.forEach(mergeResult);
          }
        })
        .catch((error) => {
          if (run === scanRun) {
            fatalError.value ??= errorMessage(error);
          }
        });

      await Promise.allSettled([fast, tasks]);
      if (run !== scanRun) {
        return;
      }
      loading.value = false;

      servicesLoading.value = true;
      await api
        .scanServices()
        .then((results) => {
          if (run === scanRun) {
            results.forEach(mergeResult);
          }
        })
        .catch((error) => {
          if (run === scanRun) {
            setSourceError("service", errorMessage(error));
          }
        })
        .finally(() => {
          if (run === scanRun) {
            servicesLoading.value = false;
          }
        });
      if (run !== scanRun) {
        return;
      }

    } finally {
      if (run === scanRun) {
        loading.value = false;
        servicesLoading.value = false;
        scanning.value = false;
        if (
          view.value === "snapshots" &&
          activeSnapshot.value &&
          diff.value
        ) {
          void loadDiff(activeSnapshot.value);
        }
      }
    }
  }

  function scanAll(): Promise<void> {
    if (scanInFlight) {
      return scanInFlight;
    }
    if (diffLoading.value || snapshotSaving.value) {
      return Promise.resolve();
    }
    const task = runScan(++scanRun).finally(() => {
      scanInFlight = null;
    });
    scanInFlight = task;
    return task;
  }

  function scanContextMenu(): Promise<void> {
    if (contextMenuInFlight) {
      return contextMenuInFlight;
    }
    const run = ++contextMenuRun;
    contextMenuLoading.value = true;
    const task = (async () => {
      try {
        const results = await api.scanContextMenu();
        if (run !== contextMenuRun) {
          return;
        }
        results.forEach(mergeResult);
        contextMenuLoaded.value = true;
        contextMenuLoadedAt.value = Date.now();
      } catch (error) {
        if (run === contextMenuRun) {
          setSourceError("contextMenu", errorMessage(error));
        }
      } finally {
        if (run === contextMenuRun) {
          contextMenuLoading.value = false;
        }
        contextMenuInFlight = null;
      }
    })();
    contextMenuInFlight = task;
    return task;
  }

  function refreshCurrentView(): Promise<void> {
    if (view.value === "snapshots") {
      return refreshSnapshots();
    }
    if (view.value === "contextMenu") {
      return scanContextMenu();
    }
    return scanAll();
  }

  async function toggle(item: StartupItem, enabled: boolean) {
    if (item.readonly || busyIds.value.has(item.id)) {
      return;
    }
    const previous = item.enabled;
    const busy = new Set(busyIds.value);
    busy.add(item.id);
    busyIds.value = busy;
    updateItem({ ...item, enabled });

    try {
      await api.setItemEnabled(item.id, item.source, enabled);
      toast.success(enabled ? "已启用" : "已禁用", {
        description: item.name,
      });
    } catch (error) {
      const current = items.value.find((entry) => entry.id === item.id);
      if (current) {
        // Roll back through updateItem so a row that gained an icon while the
        // request was in flight keeps it.
        updateItem({ ...current, enabled: previous });
      }
      const apiError = error instanceof ApiError ? error : null;
      toast.error(apiError?.message ?? "操作失败", {
        description:
          apiError?.detail ??
          (error instanceof Error ? error.message : String(error)),
      });
    } finally {
      const next = new Set(busyIds.value);
      next.delete(item.id);
      busyIds.value = next;
    }
  }

  async function saveSnapshot() {
    if (snapshotSaving.value || scanning.value || diffLoading.value) {
      return;
    }
    snapshotSaving.value = true;
    try {
      const summary = await api.saveSnapshot();
      toast.success("快照已保存", {
        description: `共 ${summary.count} 项，文件 ${summary.file}`,
      });
      try {
        const token = ++snapshotRun;
        const list = await api.listSnapshots();
        if (token === snapshotRun) {
          applySnapshotList(list);
          snapshotsLoading.value = false;
        }
      } catch {
        // The snapshot itself was saved; keep the current list and let the
        // next refresh pick it up.
        snapshotsLoading.value = false;
      }
    } catch (error) {
      const apiError = error instanceof ApiError ? error : null;
      toast.error(apiError?.message ?? "保存快照失败", {
        description: apiError?.detail ?? undefined,
      });
    } finally {
      snapshotSaving.value = false;
    }
  }

  async function refreshSnapshots() {
    if (snapshotsLoading.value) {
      return;
    }
    snapshotsLoading.value = true;
    snapshotsError.value = null;
    const token = ++snapshotRun;
    try {
      const list = await api.listSnapshots();
      if (token === snapshotRun) {
        applySnapshotList(list);
      }
    } catch (error) {
      if (token !== snapshotRun) {
        return;
      }
      const apiError = error instanceof ApiError ? error : null;
      snapshotsError.value = apiError?.detail
        ? `${apiError.message}：${apiError.detail}`
        : (apiError?.message ?? "读取快照失败");
      toast.error(apiError?.message ?? "读取快照失败", {
        description: apiError?.detail ?? undefined,
      });
    } finally {
      if (token === snapshotRun) {
        snapshotsLoading.value = false;
      }
    }
  }

  async function loadDiff(file: string) {
    if (diffLoading.value || scanning.value || snapshotSaving.value) {
      return;
    }
    const run = ++diffRun;
    diffLoading.value = true;
    activeSnapshot.value = file;
    try {
      const next = await api.diffSnapshot(file);
      if (run === diffRun) {
        diff.value = next;
      }
    } catch (error) {
      if (run !== diffRun) {
        return;
      }
      const apiError = error instanceof ApiError ? error : null;
      toast.error(apiError?.message ?? "对比快照失败", {
        description: apiError?.detail ?? undefined,
      });
      diff.value = null;
      // Show the "select a snapshot" hint instead of a blank pane.
      activeSnapshot.value = null;
    } finally {
      if (run === diffRun) {
        diffLoading.value = false;
      }
    }
  }

  async function deleteSnapshot(file: string) {
    if (deletingSnapshot.value) {
      return;
    }
    deletingSnapshot.value = file;
    try {
      await api.deleteSnapshot(file);
      snapshotRun += 1;
      // Any in-flight list request is now stale; it will not reset this flag.
      snapshotsLoading.value = false;
      if (activeSnapshot.value === file) {
        diffRun += 1;
        activeSnapshot.value = null;
        diff.value = null;
        diffLoading.value = false;
      }
      snapshots.value = snapshots.value.filter(
        (snapshot) => snapshot.file !== file,
      );
      toast.success("快照已删除");
    } catch (error) {
      const apiError = error instanceof ApiError ? error : null;
      toast.error(apiError?.message ?? "删除快照失败", {
        description: apiError?.detail ?? undefined,
      });
    } finally {
      deletingSnapshot.value = null;
    }
  }

  async function openLocation(item: StartupItem) {
    try {
      await api.openLocation(item.id, item.source);
    } catch (error) {
      const apiError = error instanceof ApiError ? error : null;
      toast.error(apiError?.message ?? "无法打开文件位置", {
        description: apiError?.detail ?? undefined,
      });
    }
  }

  function setView(next: ViewKind) {
    if (next === view.value) {
      return;
    }
    view.value = next;
    selectedId.value = null;
    if (next === "contextMenu" && !contextMenuLoading.value) {
      const stale =
        Date.now() - contextMenuLoadedAt.value > CONTEXT_MENU_STALE_MS;
      if (!contextMenuLoaded.value || stale) {
        void scanContextMenu();
      }
    }
    scheduleIcons();
  }

  function selectItem(item: StartupItem | null) {
    selectedId.value = item?.id ?? null;
  }

  onMounted(() => {
    // Start scanning immediately; a slow or failed progress listener must not
    // delay the first paint of real data. The context menu scan runs in the
    // background so its sidebar count is ready before the user opens the page.
    void scanAll();
    void scanContextMenu();
    void onScanProgress((event) => {
      progress.value = event;
    })
      .then((fn) => {
        if (disposed) {
          fn();
        } else {
          unlisten = fn;
        }
      })
      .catch(() => {
        // Progress is cosmetic.
      });
  });

  onBeforeUnmount(() => {
    disposed = true;
    unlisten?.();
    unlisten = null;
    if (iconTimer !== undefined) {
      window.clearTimeout(iconTimer);
      iconTimer = undefined;
    }
  });

  return {
    scanning,
    contextMenuLoading,
    currentLoading,
    errors,
    fatalError,
    progress,
    appState,
    view,
    filter,
    query,
    selectedId,
    selectedItem,
    busyIds,
    snapshots,
    snapshotsLoading,
    snapshotsError,
    snapshotSaving,
    deletingSnapshot,
    activeSnapshot,
    diff,
    diffLoading,
    viewItems,
    visibleItems,
    counts,
    enabledCount,
    disabledCount,
    scanAll,
    scanContextMenu,
    refreshCurrentView,
    toggle,
    saveSnapshot,
    refreshSnapshots,
    loadDiff,
    deleteSnapshot,
    openLocation,
    setView,
    selectItem,
  };
}
