<script setup lang="ts">
import { computed } from "vue";
import {
  CopyIcon,
  ExternalLinkIcon,
  PowerIcon,
  TriangleAlertIcon,
} from "@lucide/vue";
import { toast } from "vue-sonner";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Separator } from "@/components/ui/separator";
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetFooter,
  SheetHeader,
  SheetTitle,
} from "@/components/ui/sheet";
import {
  displayPath,
  scopeLabel,
  signatureLabel,
  sourceLabel,
} from "@/lib/format";
import type { StartupItem } from "@/types";

const props = defineProps<{
  item: StartupItem | null;
  open: boolean;
  busy: boolean;
}>();

const emit = defineEmits<{
  (event: "update:open", value: boolean): void;
  (event: "toggle", item: StartupItem, enabled: boolean): void;
  (event: "open-location", item: StartupItem): void;
}>();

const canOpenLocation = computed(() => {
  const item = props.item;
  if (!item) {
    return false;
  }
  return Boolean(item.exePath) || item.source === "startupFolder";
});

function copyCommand() {
  const command = props.item?.command;
  if (!command) {
    return;
  }
  if (!navigator.clipboard) {
    toast.error("当前环境不支持剪贴板");
    return;
  }
  navigator.clipboard.writeText(command).then(
    () => toast.success("命令行已复制"),
    (error) => toast.error("复制失败", { description: String(error) }),
  );
}
</script>

<template>
  <Sheet :open="open" @update:open="(value) => emit('update:open', value)">
    <SheetContent
      side="right"
      class="w-[min(480px,45vw)] gap-0 p-0 data-[side=right]:w-[min(480px,45vw)] data-[side=right]:sm:max-w-[min(480px,45vw)]"
    >
      <template v-if="item">
        <div
          :key="item.id"
          class="flex min-h-0 flex-1 flex-col animate-in fade-in duration-150"
        >
        <SheetHeader class="gap-0 border-b border-border px-5 py-5">
          <div class="flex items-start gap-3.5 pr-10">
            <div
              class="mt-0.5 flex size-10 shrink-0 items-center justify-center rounded-xl bg-muted/70 text-muted-foreground"
            >
              <img
                v-if="item.icon"
                :src="item.icon"
                alt=""
                class="size-8 object-contain animate-in fade-in duration-200"
                draggable="false"
              />
              <PowerIcon v-else class="size-5" />
            </div>
            <div class="min-w-0">
              <SheetTitle class="truncate text-[18px]">
                {{ item.name }}
              </SheetTitle>
              <SheetDescription class="mt-1.5 flex flex-wrap items-center gap-2">
                <Badge as="span" variant="outline" class="h-[22px] px-2 text-[12px]">
                  {{ sourceLabel(item.source) }}
                </Badge>
                <Badge
                  as="span"
                  :variant="item.enabled ? 'default' : 'secondary'"
                  class="h-[22px] px-2 text-[12px]"
                >
                  {{ item.enabled ? "已启用" : "已禁用" }}
                </Badge>
                <Badge
                  v-if="item.readonly"
                  as="span"
                  variant="secondary"
                  class="h-[22px] px-2 text-[12px]"
                >
                  只读
                </Badge>
              </SheetDescription>
            </div>
          </div>
        </SheetHeader>

        <div class="min-h-0 flex-1 space-y-6 overflow-y-auto px-5 py-5">
          <section class="space-y-2">
            <div class="flex items-center justify-between">
              <h3 class="text-[14px] font-medium">命令行</h3>
              <Button
                v-if="item.command"
                variant="ghost"
                size="icon-sm"
                aria-label="复制命令行"
                @click="copyCommand"
              >
                <CopyIcon />
              </Button>
            </div>
            <div
              class="rounded-lg border border-border bg-muted/40 p-3.5 font-mono text-[14px] leading-relaxed break-all whitespace-pre-wrap text-muted-foreground"
            >
              {{ item.command ?? "—" }}
            </div>
          </section>

          <Separator />

          <section class="space-y-3">
            <h3 class="text-[14px] font-medium">详细信息</h3>
            <dl class="grid grid-cols-[88px_1fr] gap-x-4 gap-y-3 text-[14px]">
              <dt class="text-muted-foreground">发布者</dt>
              <dd class="truncate">{{ item.publisher ?? "未知发布者" }}</dd>

              <dt class="text-muted-foreground">签名</dt>
              <dd>{{ signatureLabel(item.signature) }}</dd>

              <dt class="text-muted-foreground">作用域</dt>
              <dd>
                {{ scopeLabel(item.scope) }}
                <span v-if="item.view" class="text-muted-foreground">
                  · {{ item.view }} 位
                </span>
              </dd>

              <dt class="text-muted-foreground">可执行文件</dt>
              <dd class="break-words">{{ displayPath(item.exePath) }}</dd>

              <dt class="text-muted-foreground">来源</dt>
              <dd class="break-words text-muted-foreground">
                {{ displayPath(item.origin) }}
              </dd>
            </dl>
          </section>

          <section v-if="item.warnings.length" class="space-y-2">
            <h3 class="text-[14px] font-medium">注意</h3>
            <div
              class="space-y-2 rounded-lg border border-amber-500/30 bg-amber-500/10 p-3.5 text-[14px] leading-relaxed text-amber-600 dark:text-amber-400"
            >
              <div
                v-for="warning in item.warnings"
                :key="warning"
                class="flex items-start gap-1.5"
              >
                <TriangleAlertIcon class="mt-0.5 size-4 shrink-0" />
                <span>{{ warning }}</span>
              </div>
            </div>
          </section>
        </div>

        <SheetFooter class="flex-row justify-end gap-2.5 border-t border-border px-5 py-4">
          <Button
            variant="outline"
            size="sm"
            class="h-9 gap-2 px-3 text-[14px]"
            :disabled="!canOpenLocation"
            @click="emit('open-location', item)"
          >
            <ExternalLinkIcon />
            打开位置
          </Button>
          <Button
            :variant="item.enabled ? 'destructive' : 'default'"
            size="sm"
            class="h-9 gap-2 px-3 text-[14px] transition-all duration-200"
            :class="busy ? 'opacity-50' : ''"
            :disabled="item.readonly || busy"
            @click="emit('toggle', item, !item.enabled)"
          >
            <PowerIcon />
            {{ busy ? "处理中" : item.enabled ? "禁用" : "启用" }}
          </Button>
        </SheetFooter>
        </div>
      </template>
    </SheetContent>
  </Sheet>
</template>
