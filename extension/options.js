import { getBrowserApi, isGecko } from "./browser-api.js";
const api = getBrowserApi();
import { localize, t } from "./i18n.js";
const languages = await (await fetch("languages.json")).json();
const settings = await api.storage.local.get([
  "locale",
  "interceptDownloads",
  "nativeConsent",
]);
let selected = settings.locale || "en-US";
const select = document.getElementById("language");
function render() {
  const query = document
    .getElementById("language-search")
    .value.toLocaleLowerCase();
  select.replaceChildren();
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
render();
select.setAttribute("aria-label", t("language"));
document.getElementById("language-search").addEventListener("input", render);
select.addEventListener("change", async () => {
  selected = select.value;
  await api.storage.local.set({ locale: selected });
  await localize();
  document.getElementById("saved").textContent = t("saved");
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
