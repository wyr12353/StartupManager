<script setup lang="ts">
import { TriangleAlertIcon } from "@lucide/vue";
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert";
import { sourceLabel } from "@/lib/format";
import type { SourceKind } from "@/types";

defineProps<{
  errors: Partial<Record<SourceKind, string>>;
  fatalError: string | null;
}>();
</script>

<template>
  <Transition name="collapse">
    <div
      v-if="fatalError || Object.keys(errors).length"
      class="grid shrink-0 grid-rows-[1fr] bg-background"
    >
      <div class="min-h-0 overflow-hidden">
        <div class="space-y-2 border-b border-border px-5 py-3">
          <Alert v-if="fatalError" variant="destructive" class="py-3">
            <TriangleAlertIcon />
            <AlertTitle>扫描失败</AlertTitle>
            <AlertDescription>{{ fatalError }}</AlertDescription>
          </Alert>
          <Alert
            v-for="(message, source) in errors"
            :key="source"
            variant="destructive"
            class="py-3"
          >
            <TriangleAlertIcon />
            <AlertTitle>{{ sourceLabel(source as SourceKind) }}读取失败</AlertTitle>
            <AlertDescription>{{ message }}</AlertDescription>
          </Alert>
        </div>
      </div>
    </div>
  </Transition>
</template>
