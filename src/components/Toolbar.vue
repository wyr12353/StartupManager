<script setup lang="ts">
import { computed } from "vue";
import {
  CameraIcon,
  ListFilterIcon,
  RefreshCwIcon,
  SearchIcon,
} from "@lucide/vue";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import ModeToggle from "@/components/ModeToggle.vue";
import { filterLabel } from "@/lib/format";
import type { FilterMode, ViewKind } from "@/types";

const props = defineProps<{
  view: ViewKind;
  title: string;
  count: number;
  total: number;
  query: string;
  filter: FilterMode;
  loading: boolean;
  snapshotSaving: boolean;
}>();

const emit = defineEmits<{
  (event: "update:query", value: string): void;
  (event: "update:filter", value: FilterMode): void;
  (event: "refresh"): void;
  (event: "save-snapshot"): void;
}>();

const search = computed({
  get: () => props.query,
  set: (value: string) => emit("update:query", value),
});

const filters: FilterMode[] = [
  "all",
  "enabled",
  "disabled",
  "thirdParty",
  "unknownPublisher",
];
</script>

<template>
  <header
    class="flex h-16 shrink-0 items-center gap-4 border-b border-border bg-background px-5"
  >
    <div
      :key="view"
      class="flex min-w-0 items-baseline gap-2.5 animate-in fade-in duration-150"
    >
      <h1 class="truncate text-[16px] font-semibold">{{ title }}</h1>
      <span class="text-[14px] tabular-nums text-muted-foreground">
        {{ count === total ? count : `${count} / ${total}` }}
        {{ view === "snapshots" ? "个快照" : "项" }}
      </span>
    </div>

    <div v-if="view !== 'snapshots'" class="relative min-w-0 max-w-72 flex-1">
      <SearchIcon
        class="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground"
      />
      <Input
        v-model="search"
        data-search-input
        class="h-10 w-full pl-9 text-[14px]"
        placeholder="搜索名称、命令、发布者"
        aria-label="搜索启动项"
      />
    </div>

    <DropdownMenu v-if="view !== 'snapshots'">
      <DropdownMenuTrigger as-child>
        <Button variant="outline" size="sm" class="h-10 shrink-0 gap-2 px-3 text-[14px]">
          <ListFilterIcon />
          {{ filterLabel(filter) }}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="start" class="w-40">
        <DropdownMenuLabel>筛选</DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuRadioGroup
          :model-value="filter"
          @update:model-value="
            (value) => emit('update:filter', value as FilterMode)
          "
        >
          <DropdownMenuRadioItem
            v-for="mode in filters"
            :key="mode"
            :value="mode"
          >
            {{ filterLabel(mode) }}
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>

    <div class="ml-auto flex shrink-0 items-center gap-2">
      <Button
        variant="ghost"
        size="icon"
        class="size-10"
        :aria-label="view === 'snapshots' ? '刷新快照列表' : '重新扫描'"
        :disabled="loading"
        @click="emit('refresh')"
      >
        <RefreshCwIcon :class="loading ? 'animate-spin' : ''" />
      </Button>
      <ModeToggle />
      <Button
        v-if="view !== 'snapshots'"
        variant="outline"
        size="sm"
        class="h-10 gap-2 px-3 text-[14px]"
        :disabled="snapshotSaving || loading"
        @click="emit('save-snapshot')"
      >
        <CameraIcon :class="snapshotSaving ? 'animate-pulse' : ''" />
        {{ snapshotSaving ? "保存中" : "保存快照" }}
      </Button>
    </div>
  </header>
</template>
