<script setup lang="ts">
import { TriangleAlertIcon } from "@lucide/vue";
import type { AppState } from "@/types";

defineProps<{
  state: AppState | null;
}>();
</script>

<template>
  <Transition name="collapse">
    <div v-if="state?.userMismatch" class="grid shrink-0 grid-rows-[1fr]">
      <div class="min-h-0 overflow-hidden">
        <div
          class="border-b border-amber-500/30 bg-amber-500/10 px-5 py-2.5 text-[13.5px] leading-relaxed text-amber-700 dark:text-amber-300"
        >
          <div class="flex items-start gap-2">
            <TriangleAlertIcon class="mt-0.5 size-4 shrink-0" />
            <span>
              当前以
              <span class="font-medium">
                {{ state.currentUser ?? "其他管理员账户" }}
              </span>
              的管理员令牌运行，而启动本程序的账户是
              <span class="font-medium">
                {{ state.originalUser ?? "原登录账户" }}
              </span>
              。当前用户范围的启动项和右键菜单读取、修改的是管理员账户的配置；
              如需管理你自己的项目，请使用当前账户的管理员令牌重新授权。
            </span>
          </div>
        </div>
      </div>
    </div>
  </Transition>
</template>
