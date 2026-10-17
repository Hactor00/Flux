import { getBrowserApi, isGecko } from "./browser-api.js";
const api = getBrowserApi();
import { localize, t } from "./i18n.js";
import { initializeTheme } from "./ui-theme.js";
const languages = await (await fetch("languages.json")).json();
const settings = await api.storage.local.get([
  "locale",
  "interceptDownloads",
  "nativeConsent",
  "showMediaButtons",
]);
let selected = settings.locale || "";
const select = document.getElementById("language");
function render() {
  const query = document
    .getElementById("language-search")
    .value.toLocaleLowerCase();
  select.replaceChildren();
  const automatic = new Option(t("automatic"), "");
  automatic.selected = selected === "";
  select.add(automatic);
  for (const language of languages.filter((language) =>
    `${language.name} ${language.native} ${language.code}`
      .toLocaleLowerCase()
      .includes(query),
  )) {
    const option = new Option(
      `${language.native} · ${language.name}`,
      language.code,
    );
    option.selected = language.code === selected;
    select.add(option);
  }
}
await localize();
await initializeTheme(() => {
  document.getElementById("saved").textContent = t("saved");
});
document.getElementById("extension-version").textContent =
  api.runtime.getManifest().version;
document.getElementById("extension-version").title = t("extension_version");
render();
select.setAttribute("aria-label", t("language"));
document.getElementById("language-search").addEventListener("input", render);
select.addEventListener("change", async () => {
  selected = select.value;
  await api.storage.local.set({ locale: selected });
  await localize();
  render();
  document.getElementById("saved").textContent = t("saved");
});
const hud = document.getElementById("show-media-buttons");
hud.checked = settings.showMediaButtons ?? true;
hud.addEventListener("change", async () => {
  try {
    await api.storage.local.set({ showMediaButtons: hud.checked });
    document.getElementById("saved").textContent = t("saved");
  } catch (error) {
    hud.checked = !hud.checked;
    document.getElementById("saved").textContent = error.message;
  }
});
const intercept = document.getElementById("intercept");
intercept.checked = settings.interceptDownloads ?? true;
intercept.addEventListener("change", async () => {
  await api.storage.local.set({
    interceptDownloads: intercept.checked,
    interceptionConfigured: true,
  });
  document.getElementById("saved").textContent = t("saved");
});
document.getElementById("extension-id").textContent = api.runtime.id;
const consentPanel = document.getElementById("native-consent-panel");
consentPanel.hidden = !isGecko(api);
const consent = document.getElementById("native-consent");
consent.checked = settings.nativeConsent === true;
consent.addEventListener("change", async () => {
  if (consent.checked) {
    try {
      consent.checked = await api.permissions.request({
        origins: ["http://*/*", "https://*/*"],
      });
    } catch (error) {
      consent.checked = false;
      document.getElementById("saved").textContent = error.message;
    }
  }
  await api.storage.local.set({ nativeConsent: consent.checked });
  document.getElementById("saved").textContent = t("saved");
});
