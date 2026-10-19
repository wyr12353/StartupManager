import { computed, watch } from "vue";
import { useColorMode } from "@vueuse/core";
import { api } from "@/lib/api";

export type ThemeMode = "light" | "dark" | "auto";

export function useTheme() {
  const mode = useColorMode({
    storageKey: "startup-manager-theme",
    initialValue: "auto",
    selector: "html",
    attribute: "class",
    emitAuto: true,
  });

  const isDark = computed(
    () =>
      mode.value === "dark" ||
      (mode.value === "auto" && mode.system.value),
  );

  function setMode(next: ThemeMode) {
    mode.value = next;
  }

  watch(
    mode,
    (value) => {
      void api
        .setWindowTheme(value === "auto" ? "auto" : value)
        .catch(() => {});
    },
    { immediate: true },
  );

  return { mode, isDark, setMode };
}
