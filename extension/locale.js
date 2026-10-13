export const supportedLocales = [
  "en-US",
  "zh-CN",
  "hi-IN",
  "es-ES",
  "ar-SA",
  "fr-FR",
  "bn-BD",
  "pt-BR",
  "ru-RU",
  "ur-PK",
  "id-ID",
  "de-DE",
  "ja-JP",
  "pcm-NG",
  "mr-IN",
  "te-IN",
  "tr-TR",
  "ta-IN",
  "yue-HK",
  "vi-VN",
];

export function resolveLocale(value) {
  const normalized = String(value || "")
    .replaceAll("_", "-")
    .toLowerCase();
  if (/^(zh-yue|yue)(-|$)/.test(normalized)) return "yue-HK";
  const language = normalized.split("-")[0];
  return (
    supportedLocales.find((locale) => locale.toLowerCase() === normalized) ||
    supportedLocales.find((locale) => locale.split("-")[0] === language) ||
    "en-US"
  );
}

export function preferredLocale(api, stored) {
  return stored && supportedLocales.includes(stored)
    ? stored
    : resolveLocale(api.i18n?.getUILanguage?.());
}
