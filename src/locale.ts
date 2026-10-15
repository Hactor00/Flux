export function resolveLocale(value: string | null | undefined): string {
  const codes = [
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
  const normalized = (value || "").replaceAll("_", "-").toLowerCase();
  if (/^(zh-yue|yue)(-|$)/.test(normalized)) return "yue-HK";
  const language = normalized.split("-")[0];
  return (
    codes.find((code) => code.toLowerCase() === normalized) ||
    codes.find((code) => code.split("-")[0] === language) ||
    "en-US"
  );
}
