<script setup lang="ts">
import { sourceLabel } from "@/lib/format";
import type { ScanProgress } from "@/types";

defineProps<{
  mode: "items" | "snapshots";
  total: number;
  enabled: number;
  disabled: number;
  loading: boolean;
  progress: ScanProgress | null;
}>();
</script>

<template>
  <footer
    class="flex h-9 shrink-0 items-center justify-between border-t border-border px-5 text-[13.5px] text-muted-foreground"
  >
    <div class="flex items-center gap-3">
      <span>{{ mode === "snapshots" ? `共 ${total} 个快照` : `共 ${total} 项` }}</span>
      <template v-if="mode === 'items'">
        <span class="text-border">·</span>
        <span>已启用 {{ enabled }}</span>
        <span class="text-border">·</span>
        <span>已禁用 {{ disabled }}</span>
      </template>
      <Transition name="fade">
        <span v-if="loading" class="flex items-center gap-1 text-primary">
          <span class="size-1.5 animate-pulse rounded-full bg-primary" />
          正在扫描
        </span>
      </Transition>
    </div>
    <div class="flex items-center gap-4">
      <Transition name="fade">
        <span v-if="progress && loading" class="tabular-nums">
          {{ sourceLabel(progress.source) }} {{ progress.completed }}/{{
            progress.total
          }}
        </span>
      </Transition>
    </div>
  </footer>
</template>
