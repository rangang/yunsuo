let currentMode = "dark";
let media: MediaQueryList | null = null;

function render() {
  const dark =
    currentMode === "dark" ||
    (currentMode === "system" && (media?.matches ?? true));
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

export function applyTheme(mode: string) {
  currentMode = mode;
  if (!media) {
    media = window.matchMedia("(prefers-color-scheme: dark)");
    media.addEventListener("change", () => {
      if (currentMode === "system") render();
    });
  }
  render();
}