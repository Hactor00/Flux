import { getBrowserApi } from "./browser-api.js";
const api = getBrowserApi();
export let dictionary = {};
export const t = (key) => dictionary[key] || api.i18n.getMessage(key) || key;
export async function localize() {
  const response = await api.runtime.sendMessage({ type: "dictionary" });
  dictionary = response.result || {};
  const { locale = "en-US" } = await api.storage.local.get("locale");
  document.documentElement.lang = locale;
  document.documentElement.dir = /^(ar|ur)-/.test(locale) ? "rtl" : "ltr";
  for (const element of document.querySelectorAll("[data-i18n]"))
    element.textContent = t(element.dataset.i18n);
  for (const element of document.querySelectorAll("[data-i18n-placeholder]"))
    element.placeholder = t(element.dataset.i18nPlaceholder);
  for (const element of document.querySelectorAll("[data-i18n-title]")) {
    element.title = t(element.dataset.i18nTitle);
    element.setAttribute("aria-label", element.title);
  }
}
