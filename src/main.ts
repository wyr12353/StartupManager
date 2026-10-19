import { createApp } from "vue";
import { invoke } from "@tauri-apps/api/core";
import App from "./App.vue";
import "vue-sonner/style.css";
import "./assets/css/tailwind.css";

createApp(App).mount("#app");

// The main window is created hidden to avoid WebView2's white pre-paint
// frame. Show it after the themed shell has actually been painted.
requestAnimationFrame(() => {
  requestAnimationFrame(() => {
    // Give WebView2's compositor one more beat to present the first themed
    // frame before the hidden window is revealed.
    window.setTimeout(() => {
      void invoke("show_main_window").catch(() => {
        // The Rust fallback shows the window if this call fails.
      });
    }, 60);
  });
});
