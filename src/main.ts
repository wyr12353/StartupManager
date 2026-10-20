import { createApp } from "vue";
import { invoke } from "@tauri-apps/api/core";
import App from "./App.vue";
import "vue-sonner/style.css";
import "./assets/css/tailwind.css";

createApp(App).mount("#app");

// The main window is created hidden to avoid WebView2's white pre-paint
// frame. Show it after the themed shell has actually been painted.
// Hidden WebView2 windows can pause requestAnimationFrame, so use a timer
// (which still runs) to reveal the window after the first themed frame.
window.setTimeout(() => {
  void invoke("show_main_window").catch(() => {
    // The Rust fallback shows the window if this call fails.
  });
}, 80);
