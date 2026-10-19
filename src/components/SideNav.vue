<script setup lang="ts">
import {
  BoxesIcon,
  CalendarClockIcon,
  CameraIcon,
  PowerIcon,
  RocketIcon,
  ServerCogIcon,
  ShieldAlertIcon,
  ShieldCheckIcon,
} from "@lucide/vue";
import { Button } from "@/components/ui/button";
import type { ViewKind } from "@/types";

defineProps<{
  view: ViewKind;
  counts: Record<ViewKind, number>;
  version: string;
  isElevated: boolean;
}>();

const emit = defineEmits<{
  (event: "update:view", value: ViewKind): void;
}>();

const navItems = [
  { key: "login" as const, label: "登录项", icon: RocketIcon },
  { key: "tasks" as const, label: "计划任务", icon: CalendarClockIcon },
  { key: "uwp" as const, label: "UWP 应用", icon: BoxesIcon },
  { key: "services" as const, label: "服务", icon: ServerCogIcon },
  { key: "snapshots" as const, label: "快照对比", icon: CameraIcon },
];
</script>

<template>
  <aside
    class="flex w-[clamp(200px,18vw,224px)] shrink-0 flex-col border-r border-sidebar-border bg-sidebar text-sidebar-foreground"
  >
    <div class="flex h-16 items-center gap-3 px-4">
      <div
        class="flex size-10 shrink-0 items-center justify-center rounded-xl bg-primary text-primary-foreground"
      >
        <PowerIcon class="size-5" />
      </div>
      <div class="min-w-0">
        <div class="truncate text-[16px] font-semibold">启动项管理器</div>
        <div class="text-[12px] text-muted-foreground">Startup Manager</div>
      </div>
    </div>

    <nav class="flex-1 space-y-2 overflow-y-auto p-3">
      <Button
        v-for="item in navItems"
        :key="item.key"
        variant="ghost"
        size="sm"
        class="h-10 w-full justify-start gap-2.5 px-3 font-normal"
        :class="
          view === item.key
            ? 'bg-sidebar-accent text-sidebar-accent-foreground'
            : 'text-muted-foreground hover:text-foreground'
        "
        :aria-current="view === item.key ? 'page' : undefined"
        @click="emit('update:view', item.key)"
      >
        <component :is="item.icon" class="size-[18px] shrink-0" />
        <span class="truncate">{{ item.label }}</span>
        <Transition name="pop">
          <span
            v-if="counts[item.key] > 0"
            class="ml-auto rounded-full bg-muted px-2 py-0.5 text-[13px] tabular-nums text-muted-foreground"
          >
            {{ counts[item.key] }}
          </span>
        </Transition>
      </Button>
    </nav>

    <div class="space-y-2 border-t border-sidebar-border p-4">
      <div class="flex items-center gap-2 text-[14px] text-muted-foreground">
        <Transition name="icon-swap" mode="out-in">
          <ShieldCheckIcon
            v-if="isElevated"
            key="admin"
            class="size-4 text-success"
          />
          <ShieldAlertIcon v-else key="standard" class="size-4 text-warning" />
        </Transition>
        <span>{{ isElevated ? "管理员权限" : "标准权限" }}</span>
      </div>
      <div class="text-[12px] text-muted-foreground/70">版本 {{ version }}</div>
    </div>
  </aside>
</template>
