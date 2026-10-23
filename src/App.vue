<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { toast } from "vue-sonner";
import { TooltipProvider } from "@/components/ui/tooltip";
import { Toaster } from "@/components/ui/sonner";
import SideNav from "@/components/SideNav.vue";
import Toolbar from "@/components/Toolbar.vue";
import StatusBar from "@/components/StatusBar.vue";
import ErrorBanner from "@/components/ErrorBanner.vue";
import ItemTable from "@/components/ItemTable.vue";
import ItemDetails from "@/components/ItemDetails.vue";
import ConfirmDialog from "@/components/ConfirmDialog.vue";
import SnapshotPanel from "@/components/SnapshotPanel.vue";
import { useStartup } from "@/composables/useStartup";
import { isMicrosoftItem, viewLabel } from "@/lib/format";
import type { ConfirmRequest, StartupItem } from "@/types";

const {
  view,
  filter,
  query,
  selectedId,
  selectedItem,
  busyIds,
  errors,
  fatalError,
  progress,
  appState,
  scanning,
  currentLoading,
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
  toggle,
  saveSnapshot,
  refreshSnapshots,
  loadDiff,
  deleteSnapshot,
  openLocation,
  setView,
  selectItem,
} = useStartup();

const confirmRequest = ref<ConfirmRequest | null>(null);
// Keep the last item while the drawer animates closed; otherwise the sheet
// would render an empty panel during the closing transition.
const detailItem = ref<StartupItem | null>(null);
watch(selectedItem, (value) => {
  if (value) {
    detailItem.value = value;
  }
});

const detailsOpen = computed({
  get: () => selectedItem.value !== null,
  set: (value: boolean) => {
    if (!value) {
      selectItem(null);
    }
  },
});

function requestToggle(item: StartupItem, enabled: boolean) {
  if (item.readonly) {
    return;
  }
  if (item.scope === "allUsers" || isMicrosoftItem(item)) {
    confirmRequest.value = { item, enabled };
    return;
  }
  void toggle(item, enabled);
}

function clearFilters() {
  query.value = "";
  filter.value = "all";
}

function onConfirm() {
  const request = confirmRequest.value;
  confirmRequest.value = null;
  if (request) {
    void toggle(request.item, request.enabled);
  }
}

function onKeydown(event: KeyboardEvent) {
  // Rescanning remains useful while a detail sheet is open.
  if (event.key === "F5" && !event.repeat) {
    event.preventDefault();
    if (snapshotSaving.value || diffLoading.value) {
      toast.info("正在保存或对比快照，请稍候");
      return;
    }
    void scanAll();
    return;
  }
  // Let open dialogs/sheets handle their own keys (Escape, focus trap).
  if (document.querySelector('[role="dialog"], [role="alertdialog"]')) {
    return;
  }
  const modifier = event.ctrlKey || event.metaKey;
  if (modifier && event.key.toLowerCase() === "f") {
    const input = document.querySelector<HTMLInputElement>("[data-search-input]");
    if (input) {
      event.preventDefault();
      input.focus();
    }
  }
}

watch(scanning, (value) => {
  if (value) {
    confirmRequest.value = null;
  }
});

onMounted(() => {
  window.addEventListener("keydown", onKeydown);
});
onBeforeUnmount(() => {
  window.removeEventListener("keydown", onKeydown);
});
</script>

<template>
  <TooltipProvider :delay-duration="300">
    <div class="flex h-full min-h-0 overflow-hidden bg-background text-foreground">
      <SideNav
        :view="view"
        :counts="counts"
        :version="appState?.version ?? '0.1.1'"
        :is-elevated="appState ? appState.isElevated : null"
        @update:view="setView"
      />

      <div class="flex min-w-0 flex-1 flex-col">
        <Toolbar
          :view="view"
          :title="viewLabel(view)"
          :count="view === 'snapshots' ? counts.snapshots : visibleItems.length"
          :total="view === 'snapshots' ? counts.snapshots : viewItems.length"
          :query="query"
          :filter="filter"
          :loading="
            currentLoading ||
            scanning ||
            snapshotSaving ||
            diffLoading ||
            (view === 'snapshots' && snapshotsLoading)
          "
          :snapshot-saving="snapshotSaving"
          @update:query="query = $event"
          @update:filter="filter = $event"
          @refresh="view === 'snapshots' ? refreshSnapshots() : scanAll()"
          @save-snapshot="saveSnapshot"
        />

        <ErrorBanner :errors="errors" :fatal-error="fatalError" />

        <Transition name="collapse">
          <div
            v-if="view === 'services'"
            class="grid shrink-0 grid-rows-[1fr]"
          >
            <div class="min-h-0 overflow-hidden">
              <div
                class="border-b border-border bg-muted/30 px-5 py-2.5 text-[14px] text-muted-foreground"
              >
                服务列表为只读，工具暂不提供启用/禁用，避免误改系统关键服务。
              </div>
            </div>
          </div>
        </Transition>

        <div class="min-h-0 flex-1">
          <Transition name="view" mode="out-in">
            <SnapshotPanel
              v-if="view === 'snapshots'"
              key="snapshots"
              :snapshots="snapshots"
              :snapshots-loading="snapshotsLoading"
              :error="snapshotsError"
              :scanning="scanning"
              :deleting="deletingSnapshot"
              :diff="diff"
              :active-snapshot="activeSnapshot"
              :diff-loading="diffLoading"
              :saving="snapshotSaving"
              @save="saveSnapshot"
              @load-diff="loadDiff"
              @delete="deleteSnapshot"
            />
            <ItemTable
              v-else
              key="items"
              :items="visibleItems"
              :busy-ids="busyIds"
              :selected-id="selectedId"
              :loading="currentLoading"
              :has-filters="Boolean(query) || filter !== 'all'"
              @select="selectItem"
              @toggle="requestToggle"
              @refresh="scanAll"
              @clear-filters="clearFilters"
            />
          </Transition>
        </div>

        <StatusBar
          :mode="view === 'snapshots' ? 'snapshots' : 'items'"
          :total="view === 'snapshots' ? counts.snapshots : viewItems.length"
          :enabled="enabledCount"
          :disabled="disabledCount"
          :loading="
            currentLoading ||
            scanning ||
            snapshotSaving ||
            diffLoading ||
            (view === 'snapshots' && snapshotsLoading)
          "
          :progress="progress"
        />
      </div>
    </div>

    <ItemDetails
      :item="detailItem"
      :open="detailsOpen"
      :busy="selectedItem ? busyIds.has(selectedItem.id) : false"
      @update:open="detailsOpen = $event"
      @toggle="requestToggle"
      @open-location="openLocation"
    />

    <ConfirmDialog
      :request="confirmRequest"
      @update:open="
        (value) => {
          if (!value) confirmRequest = null;
        }
      "
      @confirm="onConfirm"
    />

    <Toaster
      position="bottom-right"
      :offset="{ bottom: 48, right: 20 }"
      close-button
    />
  </TooltipProvider>
</template>
