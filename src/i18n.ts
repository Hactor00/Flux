import i18n from "i18next";
import { initReactI18next } from "react-i18next";
export const languages = [
  { code: "en-US", name: "English", native: "English" },
  { code: "zh-CN", name: "Mandarin Chinese", native: "普通话" },
  { code: "hi-IN", name: "Hindi", native: "हिन्दी" },
  { code: "es-ES", name: "Spanish", native: "Español" },
  { code: "ar-SA", name: "Standard Arabic", native: "العربية" },
  { code: "fr-FR", name: "French", native: "Français" },
  { code: "bn-BD", name: "Bengali", native: "বাংলা" },
  { code: "pt-BR", name: "Portuguese", native: "Português" },
  { code: "ru-RU", name: "Russian", native: "Русский" },
  { code: "ur-PK", name: "Urdu", native: "اردو" },
  { code: "id-ID", name: "Indonesian", native: "Bahasa Indonesia" },
  { code: "de-DE", name: "German", native: "Deutsch" },
  { code: "ja-JP", name: "Japanese", native: "日本語" },
  { code: "pcm-NG", name: "Nigerian Pidgin", native: "Naijá" },
  { code: "mr-IN", name: "Marathi", native: "मराठी" },
  { code: "te-IN", name: "Telugu", native: "తెలుగు" },
  { code: "tr-TR", name: "Turkish", native: "Türkçe" },
  { code: "ta-IN", name: "Tamil", native: "தமிழ்" },
  { code: "yue-HK", name: "Yue Chinese", native: "粵語" },
  { code: "vi-VN", name: "Vietnamese", native: "Tiếng Việt" },
];
const modules = import.meta.glob<{ default: Record<string, string> }>(
  "./locales/*.json",
  { eager: true },
);
const resources = Object.fromEntries(
  languages.map((language) => [
    language.code,
    { translation: modules[`./locales/${language.code}.json`].default },
  ]),
);
const saved = localStorage.getItem("flux-language");
void i18n.use(initReactI18next).init({
  resources,
  lng: languages.some((l) => l.code === saved) ? saved! : "en-US",
  fallbackLng: "en-US",
  supportedLngs: languages.map((l) => l.code),
  interpolation: { escapeValue: false },
  returnNull: false,
});
export default i18n;
