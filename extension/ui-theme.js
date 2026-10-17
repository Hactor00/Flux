import { getBrowserApi } from "./browser-api.js";
export async function initializeTheme(onSave = () => {}) {
  const api = getBrowserApi();
  const media = document.defaultView?.matchMedia?.(
    "(prefers-color-scheme: light)",
  );
  let preference = "system";
  function apply(value) {
    preference = ["light", "dark"].includes(value) ? value : "system";
    document.documentElement.dataset.theme =
      preference === "system"
        ? media?.matches
          ? "light"
          : "dark"
        : preference;
    for (const button of document.querySelectorAll("[data-theme-choice]"))
      button.setAttribute(
        "aria-pressed",
        String(button.dataset.themeChoice === preference),
      );
  }
  apply((await api.storage.local.get("extensionTheme")).extensionTheme);
  media?.addEventListener("change", () => apply(preference));
  api.storage.onChanged?.addListener((changes, area) => {
    if (area === "local" && changes.extensionTheme)
      apply(changes.extensionTheme.newValue);
  });
  for (const button of document.querySelectorAll("[data-theme-choice]"))
    button.addEventListener("click", async () => {
      try {
        await api.storage.local.set({
          extensionTheme: button.dataset.themeChoice,
        });
        apply(button.dataset.themeChoice);
        onSave();
      } catch (error) {
        const status = document.getElementById("saved");
        if (status) status.textContent = error.message;
      }
    });
}
