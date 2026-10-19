<script setup lang="ts">
import { MoonIcon, SunIcon, MonitorIcon } from "@lucide/vue";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useTheme, type ThemeMode } from "@/composables/useTheme";

const { mode, isDark, setMode } = useTheme();

const options = [
  { value: "light" as const, label: "浅色", icon: SunIcon },
  { value: "dark" as const, label: "深色", icon: MoonIcon },
  { value: "auto" as const, label: "跟随系统", icon: MonitorIcon },
];
</script>

<template>
  <DropdownMenu>
    <DropdownMenuTrigger as-child>
      <Button variant="ghost" size="icon" class="size-10" aria-label="切换主题">
        <Transition name="icon-swap" mode="out-in">
          <MoonIcon v-if="isDark" key="moon" />
          <SunIcon v-else key="sun" />
        </Transition>
      </Button>
    </DropdownMenuTrigger>
    <DropdownMenuContent align="end" class="w-36">
      <DropdownMenuRadioGroup
        :model-value="mode"
        @update:model-value="(value) => setMode(value as ThemeMode)"
      >
        <DropdownMenuRadioItem
          v-for="option in options"
          :key="option.value"
          :value="option.value"
        >
          <component :is="option.icon" class="size-4" />
          {{ option.label }}
        </DropdownMenuRadioItem>
      </DropdownMenuRadioGroup>
    </DropdownMenuContent>
  </DropdownMenu>
</template>
