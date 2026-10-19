(function () {
  try {
    var raw = localStorage.getItem("startup-manager-theme") || "auto";
    var mode = raw.replace(/^"|"$/g, "");
    var dark =
      mode === "dark" ||
      (mode !== "light" &&
        window.matchMedia("(prefers-color-scheme: dark)").matches);
    document.documentElement.classList.toggle("dark", dark);
  } catch (_) {
    document.documentElement.classList.add("dark");
  }
})();
