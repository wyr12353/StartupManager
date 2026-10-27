<script setup lang="ts">
import { computed } from "vue";
import { TriangleAlertIcon } from "@lucide/vue";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogMedia,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import { Button } from "@/components/ui/button";
import { isContextMenuHandler, isMicrosoftItem } from "@/lib/format";
import type { ConfirmRequest } from "@/types";

const props = defineProps<{
  request: ConfirmRequest | null;
}>();

const emit = defineEmits<{
  (event: "update:open", value: boolean): void;
  (event: "confirm"): void;
}>();

const title = computed(() => {
  if (!props.request) {
    return "";
  }
  const verb = props.request.enabled ? "启用" : "禁用";
  return `确定要${verb}“${props.request.item.name}”吗？`;
});

const description = computed(() => {
  const request = props.request;
  if (!request) {
    return "";
  }
  if (request.item.source === "service") {
    return "服务项为只读，工具不会修改。";
  }
  if (request.item.source === "contextMenu") {
    const parts: string[] = [
      request.enabled
        ? "启用后会重新出现在右键菜单中。"
        : "禁用后会从右键菜单隐藏，随时可以重新启用。",
    ];
    if (isContextMenuHandler(request.item)) {
      parts.push("这是 COM 扩展，禁用会影响所有用户和所有注册位置。");
    }
    if (request.item.machineWide) {
      parts.push("这是机器级注册项，修改会影响所有用户，需要管理员授权。");
    }
    if (isMicrosoftItem(request.item)) {
      parts.push("这是 Microsoft 系统项，修改可能影响系统功能。");
    }
    return parts.join(" ");
  }
  const parts: string[] = [];
  if (request.item.source === "scheduledTask") {
    parts.push(
      request.enabled
        ? "启用后会在下次触发时运行。"
        : "禁用后会在下次触发时跳过，随时可以重新启用。",
    );
    parts.push("计划任务由系统统一管理，通常立即生效。");
    if (request.item.machineWide) {
      parts.push("这是机器级任务，修改需要管理员授权。");
    }
  } else {
    parts.push(
      request.enabled
        ? "启用后会在下次登录时运行。"
        : "禁用后会在下次登录时跳过，随时可以重新启用。",
    );
    if (request.item.machineWide) {
      parts.push("这是机器级启动项，修改会影响所有用户，需要管理员授权。");
    }
  }
  if (isMicrosoftItem(request.item)) {
    parts.push("这是 Microsoft 系统项，修改可能影响系统功能。");
  }
  return parts.join(" ");
});
</script>

<template>
  <AlertDialog
    :open="Boolean(request)"
    @update:open="(value) => emit('update:open', value)"
  >
    <AlertDialogContent class="p-7">
      <AlertDialogHeader>
        <AlertDialogMedia
          class="size-12 bg-destructive/10 text-destructive"
        >
          <TriangleAlertIcon class="size-6" />
        </AlertDialogMedia>
        <AlertDialogTitle class="text-[18px]">{{ title }}</AlertDialogTitle>
        <AlertDialogDescription class="text-[14px] leading-relaxed">
          {{ description }}
        </AlertDialogDescription>
      </AlertDialogHeader>
      <AlertDialogFooter>
        <AlertDialogCancel class="h-10 px-4 text-[14px]">取消</AlertDialogCancel>
        <!-- AlertDialogAction is a DialogClose: it clears the pending request
             before the confirm handler runs. A plain button lets the parent
             close the dialog after starting the action. -->
        <Button
          :class="
            request && !request.enabled
              ? 'h-10 bg-destructive px-4 text-[14px] text-white hover:bg-destructive/90'
              : 'h-10 px-4 text-[14px]'
          "
          @click="emit('confirm')"
        >
          {{ request?.enabled ? "启用" : "禁用" }}
        </Button>
      </AlertDialogFooter>
    </AlertDialogContent>
  </AlertDialog>
</template>
