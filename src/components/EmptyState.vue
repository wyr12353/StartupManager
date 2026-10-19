<script setup lang="ts">
import type { Component } from "vue";
import { BoxesIcon, RefreshCwIcon } from "@lucide/vue";
import { Button } from "@/components/ui/button";

withDefaults(
  defineProps<{
    title: string;
    description: string;
    actionLabel?: string;
    actionIcon?: Component;
  }>(),
  {
    actionLabel: "",
    actionIcon: () => RefreshCwIcon,
  },
);

const emit = defineEmits<{
  (event: "action"): void;
}>();
</script>

<template>
  <div class="flex h-full flex-col items-center justify-center gap-4 px-6 py-12 text-center">
    <div
      class="flex size-14 items-center justify-center rounded-2xl border border-border bg-muted/50 text-muted-foreground"
    >
      <BoxesIcon class="size-6" />
    </div>
    <div class="space-y-1.5">
      <div class="text-[17px] font-medium">{{ title }}</div>
      <div class="max-w-md text-[14px] text-muted-foreground">
        {{ description }}
      </div>
    </div>
    <Button
      v-if="actionLabel"
      variant="outline"
      size="sm"
      class="h-9 gap-2 text-[14px]"
      @click="emit('action')"
    >
      <component :is="actionIcon" />
      {{ actionLabel }}
    </Button>
  </div>
</template>
