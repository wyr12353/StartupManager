<script setup lang="ts">
import { ref } from "vue";
import {
  ArrowRightIcon,
  CameraIcon,
  MinusIcon,
  PlusIcon,
  Trash2Icon,
} from "@lucide/vue";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import EmptyState from "@/components/EmptyState.vue";
import {
  formatDateTime,
  shortCommand,
  sourceShortLabel,
} from "@/lib/format";
import type { ItemChange, SnapshotDiff, SnapshotSummary } from "@/types";

defineProps<{
  snapshots: SnapshotSummary[];
  snapshotsLoading: boolean;
  error: string | null;
  scanning: boolean;
  deleting: string | null;
  diff: SnapshotDiff | null;
  activeSnapshot: string | null;
  diffLoading: boolean;
  saving: boolean;
}>();

const emit = defineEmits<{
  (event: "save"): void;
  (event: "load-diff", file: string): void;
  (event: "delete", file: string): void;
}>();

const pendingDelete = ref<string | null>(null);
const deleteOpen = ref(false);

function askDelete(file: string) {
  pendingDelete.value = file;
  deleteOpen.value = true;
}

function confirmDelete() {
  const file = pendingDelete.value;
  deleteOpen.value = false;
  pendingDelete.value = null;
  if (file) {
    emit("delete", file);
  }
}

function changeLabel(change: ItemChange): string {
  if (change.beforeEnabled !== change.afterEnabled) {
    return "启用状态";
  }
  if (change.beforeCommand !== change.afterCommand) {
    return "命令行";
  }
  return "名称";
}
</script>

<template>
  <div class="flex h-full min-h-0">
    <aside
      class="flex w-[clamp(240px,24vw,300px)] shrink-0 flex-col border-r border-border bg-sidebar/40"
    >
      <div class="flex h-12 items-center border-b border-border px-5">
        <span class="text-[14px] font-medium">历史快照</span>
      </div>

      <TransitionGroup
        name="list"
        tag="div"
        class="relative min-h-0 flex-1 overflow-y-auto p-2"
      >
        <div
          v-for="snapshot in snapshots"
          :key="snapshot.file"
          class="group relative mb-1.5 rounded-lg transition-colors"
          :class="
            activeSnapshot === snapshot.file
              ? 'bg-muted'
              : 'hover:bg-muted/60'
          "
        >
          <button
            type="button"
            class="w-full rounded-lg px-3 py-3 pr-10 text-left transition-colors disabled:cursor-not-allowed disabled:opacity-60"
            :class="
              activeSnapshot === snapshot.file
                ? 'text-foreground'
                : 'text-muted-foreground hover:text-foreground'
            "
            :aria-current="activeSnapshot === snapshot.file ? 'true' : undefined"
            :disabled="diffLoading || scanning || saving"
            @click="emit('load-diff', snapshot.file)"
          >
            <span class="block text-[14px] font-medium">
              {{ formatDateTime(snapshot.createdAt) }}
            </span>
            <span class="mt-1 block truncate text-[12.5px] text-muted-foreground">
              {{ snapshot.count }} 项 · {{ snapshot.file }}
            </span>
          </button>
          <Button
            variant="ghost"
            size="icon-sm"
            class="absolute top-1.5 right-1.5 size-7 text-muted-foreground opacity-60 transition-opacity hover:text-destructive group-hover:opacity-100"
            :disabled="Boolean(deleting)"
            :aria-busy="deleting === snapshot.file"
            :aria-label="`删除快照 ${formatDateTime(snapshot.createdAt)}`"
            @click.stop="askDelete(snapshot.file)"
          >
            <Trash2Icon class="size-4" />
          </Button>
        </div>

        <div
          v-if="snapshotsLoading && snapshots.length === 0 && !error"
          key="snapshots-loading"
          class="space-y-2 px-3 py-3"
        >
          <Skeleton class="h-12 w-full" />
          <Skeleton class="h-12 w-full" />
          <Skeleton class="h-12 w-full" />
        </div>
        <div
          v-else-if="error && snapshots.length === 0"
          key="error"
          class="px-3 py-8 text-center text-[13px] leading-relaxed text-destructive"
        >
          {{ error }}
        </div>
        <div
          v-else-if="snapshots.length === 0"
          key="empty"
          class="px-3 py-8 text-center text-[14px] text-muted-foreground"
        >
          还没有保存过快照
        </div>
      </TransitionGroup>

      <div class="p-2">
        <Button
          variant="outline"
          size="sm"
          class="h-9 w-full gap-2 text-[14px]"
          :disabled="saving || scanning || diffLoading"
          @click="emit('save')"
        >
          <CameraIcon :class="saving ? 'animate-pulse' : ''" />
          {{ saving ? "保存中" : "保存当前快照" }}
        </Button>
      </div>
    </aside>

    <section class="flex min-w-0 flex-1 flex-col">
      <div
        class="flex h-12 shrink-0 items-center justify-between border-b border-border px-5"
      >
        <span class="text-[14px] font-medium">差异详情</span>
        <Transition name="fade" mode="out-in">
          <span
            v-if="activeSnapshot"
            :key="activeSnapshot"
            class="max-w-[55%] truncate text-[12px] text-muted-foreground"
          >
            {{ activeSnapshot }}
          </span>
        </Transition>
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto">
        <div
          v-if="error"
          class="m-5 rounded-lg border border-destructive/30 bg-destructive/10 p-3.5 text-[14px] leading-relaxed text-destructive"
        >
          {{ error }}
        </div>
        <Transition name="fade" mode="out-in">
        <div
          v-if="snapshotsLoading && !diff"
          key="snapshots-loading"
          class="space-y-4 p-5"
        >
          <Skeleton class="h-10 w-72" />
          <Skeleton class="h-24 w-full" />
          <Skeleton class="h-24 w-full" />
        </div>

        <div v-else-if="snapshots.length === 0" key="empty" class="h-full">
        <EmptyState
          title="用快照找出新出现的启动项"
          description="保存一次当前状态，以后安装软件或更新后再次对比，就能看到这次多了什么、什么被改了。"
          action-label="保存当前快照"
          :action-icon="CameraIcon"
          :disabled="saving || scanning || diffLoading"
          @action="emit('save')"
        />
      </div>

      <div
        v-else-if="!activeSnapshot"
        key="prompt"
        class="flex h-full items-center justify-center px-6 text-center text-[14px] text-muted-foreground"
      >
        选择左侧的快照，查看它与当前状态的差异
      </div>

      <div v-else-if="diffLoading" key="loading" class="space-y-4 p-5">
        <Skeleton class="h-10 w-72" />
        <Skeleton class="h-24 w-full" />
        <Skeleton class="h-24 w-full" />
      </div>

      <div
        v-else-if="diff"
        :key="activeSnapshot ?? 'diff'"
        class="space-y-6 p-5"
      >
        <div class="grid grid-cols-3 gap-4">
          <div class="rounded-lg border border-border bg-card p-4">
            <div class="flex items-center gap-2 text-[12.5px] text-muted-foreground">
              <PlusIcon class="size-4 text-success" />
              新增
            </div>
            <div class="mt-1.5 text-2xl font-semibold tabular-nums">
              {{ diff.added.length }}
            </div>
          </div>
          <div class="rounded-lg border border-border bg-card p-4">
            <div class="flex items-center gap-2 text-[12.5px] text-muted-foreground">
              <MinusIcon class="size-4 text-destructive" />
              移除
            </div>
            <div class="mt-1.5 text-2xl font-semibold tabular-nums">
              {{ diff.removed.length }}
            </div>
          </div>
          <div class="rounded-lg border border-border bg-card p-4">
            <div class="flex items-center gap-2 text-[12.5px] text-muted-foreground">
              <ArrowRightIcon class="size-4 text-primary" />
              变更项
            </div>
            <div class="mt-1.5 text-2xl font-semibold tabular-nums">
              {{ diff.changed.length }}
            </div>
          </div>
        </div>

        <section v-if="diff.added.length" class="space-y-2">
          <h3 class="text-[14px] font-medium text-success">新增启动项</h3>
          <div class="divide-y divide-border rounded-lg border border-border bg-card">
            <div
              v-for="item in diff.added"
              :key="item.id"
              class="flex items-center gap-4 px-4 py-3"
            >
              <div class="min-w-0 flex-1">
                <div class="truncate text-[14px]">{{ item.name }}</div>
                <div class="truncate font-mono text-[13px] text-muted-foreground">
                  {{ shortCommand(item.command, 72) }}
                </div>
              </div>
              <Badge variant="outline" class="h-[22px] px-2 text-[12px]">
                {{ sourceShortLabel(item.source) }}
              </Badge>
            </div>
          </div>
        </section>

        <section v-if="diff.changed.length" class="space-y-2">
          <h3 class="text-[14px] font-medium text-primary">变更项</h3>
          <div class="divide-y divide-border rounded-lg border border-border bg-card">
            <div
              v-for="change in diff.changed"
              :key="change.id"
              class="flex items-center gap-4 px-4 py-3"
            >
              <div class="min-w-0 flex-1">
                <div class="truncate text-[14px]">{{ change.name }}</div>
                <div class="mt-0.5 text-[13px] text-muted-foreground">
                  <span
                    v-if="change.beforeName !== change.name"
                    class="mr-2 text-muted-foreground/70 line-through"
                  >
                    {{ change.beforeName }}
                  </span>
                  <template v-if="change.beforeEnabled !== change.afterEnabled">
                    {{ change.beforeEnabled ? "已启用" : "已禁用" }}
                    <ArrowRightIcon class="mx-0.5 inline size-3.5" />
                    {{ change.afterEnabled ? "已启用" : "已禁用" }}
                  </template>
                  <template v-if="change.beforeCommand !== change.afterCommand">
                    <div class="break-all font-mono text-[12.5px] line-through opacity-60">
                      {{ shortCommand(change.beforeCommand, 72) }}
                    </div>
                    <div class="break-all font-mono text-[12.5px]">
                      {{ shortCommand(change.afterCommand, 72) }}
                    </div>
                  </template>
                </div>
              </div>
              <Badge variant="outline" class="h-[22px] px-2 text-[12px]">
                {{ changeLabel(change) }}
              </Badge>
            </div>
          </div>
        </section>

        <section v-if="diff.removed.length" class="space-y-2">
          <h3 class="text-[14px] font-medium text-destructive">移除的启动项</h3>
          <div class="divide-y divide-border rounded-lg border border-border bg-card">
            <div
              v-for="item in diff.removed"
              :key="item.id"
              class="flex items-center gap-4 px-4 py-3"
            >
              <div class="min-w-0 flex-1">
                <div class="truncate text-[14px]">{{ item.name }}</div>
                <div class="truncate font-mono text-[13px] text-muted-foreground">
                  {{ shortCommand(item.command, 72) }}
                </div>
              </div>
              <Badge variant="outline" class="h-[22px] px-2 text-[12px]">
                {{ sourceShortLabel(item.source) }}
              </Badge>
            </div>
          </div>
        </section>

        <div
          v-if="
            diff.added.length === 0 &&
            diff.removed.length === 0 &&
            diff.changed.length === 0
          "
          class="rounded-lg border border-dashed border-border px-4 py-10 text-center text-[14px] text-muted-foreground"
        >
          与当前状态一致，没有发现变化
        </div>
      </div>
        </Transition>
      </div>
    </section>

    <AlertDialog
      :open="deleteOpen"
      @update:open="deleteOpen = $event"
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>删除这条快照？</AlertDialogTitle>
          <AlertDialogDescription>
            快照文件会从本机删除，且无法恢复。启动项本身不会受到影响。
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel @click="pendingDelete = null">取消</AlertDialogCancel>
          <AlertDialogAction
            class="bg-destructive text-white hover:bg-destructive/90"
            @click="confirmDelete"
          >
            删除
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  </div>
</template>
