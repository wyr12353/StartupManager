<script setup lang="ts">
import {
  BoxesIcon,
  CalendarClockIcon,
  FolderIcon,
  RocketIcon,
  ServerCogIcon,
  TriangleAlertIcon,
} from "@lucide/vue";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import EmptyState from "@/components/EmptyState.vue";
import {
  scopeLabel,
  shortCommand,
  sourceShortLabel,
} from "@/lib/format";
import type { SourceKind, StartupItem } from "@/types";

defineProps<{
  items: StartupItem[];
  busyIds: Set<string>;
  selectedId: string | null;
  loading: boolean;
  hasFilters: boolean;
}>();

const emit = defineEmits<{
  (event: "select", item: StartupItem): void;
  (event: "toggle", item: StartupItem, enabled: boolean): void;
  (event: "refresh"): void;
  (event: "clear-filters"): void;
}>();

function iconFor(source: SourceKind) {
  switch (source) {
    case "registryRun":
      return RocketIcon;
    case "startupFolder":
      return FolderIcon;
    case "uwp":
      return BoxesIcon;
    case "scheduledTask":
      return CalendarClockIcon;
    case "service":
      return ServerCogIcon;
  }
}

</script>

<template>
  <div class="h-full overflow-auto">
    <Transition name="fade" mode="out-in">
      <div
        v-if="loading && items.length === 0"
        key="loading"
        class="space-y-2 p-4"
      >
        <div v-for="index in 8" :key="index" class="flex items-center gap-3">
          <Skeleton class="size-7 rounded-md" />
          <Skeleton class="h-4 w-48" />
          <Skeleton class="ml-auto h-4 w-16" />
        </div>
      </div>

      <EmptyState
        v-else-if="items.length === 0"
        key="empty"
        :title="hasFilters ? '没有匹配的启动项' : '没有找到启动项'"
        :description="
          hasFilters
            ? '换个关键词，或清除搜索和筛选条件。'
            : '可以重新扫描一次，或检查当前来源是否可读。'
        "
        :action-label="hasFilters ? '清除搜索和筛选' : '重新扫描'"
        @action="hasFilters ? emit('clear-filters') : emit('refresh')"
      />

      <Table v-else key="table" class="w-full table-fixed">
      <TableHeader class="bg-background [&_th]:sticky [&_th]:top-0 [&_th]:z-10 [&_th]:bg-background">
        <TableRow class="hover:bg-transparent">
          <TableHead scope="col" class="h-12 w-[30%] pl-5 pr-4 text-[14px]">名称</TableHead>
          <TableHead scope="col" class="h-12 w-[17%] px-4 text-[14px]">发布者</TableHead>
          <TableHead scope="col" class="hidden h-12 w-[23%] px-4 text-[14px] lg:table-cell">命令</TableHead>
          <TableHead scope="col" class="h-12 w-[14%] pl-6 pr-4 text-[14px]">来源</TableHead>
          <TableHead scope="col" class="h-12 w-[16%] pl-4 pr-5 text-right text-[14px]">状态</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        <TableRow
          v-for="item in items"
          :key="item.id"
          class="h-16 cursor-pointer animate-in fade-in duration-150"
          :class="selectedId === item.id ? 'bg-muted/60' : ''"
          @click="emit('select', item)"
        >
          <TableCell class="pl-5 pr-4">
            <div class="flex min-w-0 items-center gap-2.5">
              <div
                class="flex size-8 shrink-0 items-center justify-center rounded-lg bg-muted/70 text-muted-foreground"
              >
                <img
                  v-if="item.icon"
                  :src="item.icon"
                  alt=""
                  class="size-7 object-contain animate-in fade-in duration-200"
                  draggable="false"
                />
                <component v-else :is="iconFor(item.source)" class="size-4" />
              </div>
              <div class="min-w-0 flex-1">
                <button
                  type="button"
                  class="block w-full truncate text-left text-[15px] font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
                  :aria-current="
                    selectedId === item.id ? 'true' : undefined
                  "
                  @click.stop="emit('select', item)"
                >
                  {{ item.name }}
                </button>
                <div class="truncate text-[13px] text-muted-foreground">
                  {{ scopeLabel(item.scope) }}
                  <span v-if="item.view"> · {{ item.view }} 位</span>
                </div>
              </div>
              <div class="flex w-6 shrink-0 items-center justify-center">
                <Tooltip
                  v-if="item.warnings.length"
                  :disable-closing-trigger="true"
                >
                  <TooltipTrigger as-child>
                    <button
                      type="button"
                      class="inline-flex size-5 items-center justify-center rounded-sm text-warning outline-none focus-visible:ring-2 focus-visible:ring-ring"
                      :aria-label="`查看警告：${item.warnings.join('；')}`"
                      @click.stop
                    >
                      <TriangleAlertIcon class="size-4" />
                    </button>
                  </TooltipTrigger>
                  <TooltipContent
                    side="top"
                    class="max-w-sm items-start text-[13px] leading-relaxed"
                  >
                    <div class="space-y-1.5">
                      <div
                        v-for="warning in item.warnings"
                        :key="warning"
                        class="flex items-start gap-1.5"
                      >
                        <span
                          class="mt-[6px] size-1 shrink-0 rounded-full bg-muted-foreground/60"
                        />
                        <span>{{ warning }}</span>
                      </div>
                    </div>
                  </TooltipContent>
                </Tooltip>
              </div>
            </div>
          </TableCell>

          <TableCell class="px-4">
            <span class="block truncate text-[14px] text-muted-foreground">
              {{ item.publisher ?? "未知发布者" }}
            </span>
          </TableCell>

          <TableCell class="hidden px-4 lg:table-cell">
            <span
              class="block truncate font-mono text-[12.5px] text-muted-foreground"
              :title="item.command ?? ''"
            >
              {{ shortCommand(item.command, 88) }}
            </span>
          </TableCell>

          <TableCell class="pl-6 pr-4">
            <div class="flex items-center gap-1.5">
              <Badge variant="outline" class="h-[22px] px-2 text-[12px]">
                {{ sourceShortLabel(item.source) }}
              </Badge>
              <Badge
                v-if="item.readonly"
                variant="secondary"
                class="h-[22px] px-2 text-[11px]"
              >
                只读
              </Badge>
            </div>
          </TableCell>

          <TableCell class="pl-4 pr-5 text-right">
            <div class="flex items-center justify-end gap-2.5">
              <span
                class="inline-flex h-[22px] w-11 items-center justify-end text-[13px] leading-none transition-colors duration-200"
                :class="
                  item.enabled ? 'text-success' : 'text-muted-foreground'
                "
              >
                {{ item.enabled ? "已启用" : "已禁用" }}
              </span>
              <div
                class="flex h-[22px] items-center transition-all duration-200"
                @click.stop
                :class="
                  busyIds.has(item.id)
                    ? 'pointer-events-none opacity-50 delay-150'
                    : ''
                "
              >
                <Switch
                  :model-value="item.enabled"
                  :disabled="item.readonly || busyIds.has(item.id)"
                  :aria-busy="busyIds.has(item.id)"
                  :aria-label="`${item.enabled ? '禁用' : '启用'} ${item.name}`"
                  :title="
                    item.readonly ? '该启动项为只读，工具不会修改' : undefined
                  "
                  @update:model-value="
                    (value) => emit('toggle', item, Boolean(value))
                  "
                />
              </div>
            </div>
          </TableCell>
        </TableRow>
      </TableBody>
    </Table>
    </Transition>
  </div>
</template>
