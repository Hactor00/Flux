export type Theme = "system" | "dark" | "light";
export function applyTheme(theme: Theme) {
  const light =
    theme === "light" ||
    (theme === "system" && matchMedia("(prefers-color-scheme: light)").matches);
  document.documentElement.dataset.theme = light ? "light" : "dark";
  localStorage.setItem("flux-theme", theme);
}
export function storedTheme(): Theme {
  const theme = localStorage.getItem("flux-theme");
  return theme === "light" || theme === "dark" ? theme : "system";
}
