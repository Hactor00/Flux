import test from "node:test";
import assert from "node:assert/strict";
import { resolveLocale as desktopLocale } from "../src/locale.ts";
import { resolveLocale, preferredLocale } from "../extension/locale.js";

test("desktop and browser locales match regional variants and use English fallback", () => {
  for (const [input, expected] of [
    ["tr", "tr-TR"],
    ["tr-CY", "tr-TR"],
    ["pt-PT", "pt-BR"],
    ["es-MX", "es-ES"],
    ["zh-Hant-TW", "zh-CN"],
    ["zh-yue-HK", "yue-HK"],
    ["ko-KR", "en-US"],
    ["", "en-US"],
  ]) {
    assert.equal(resolveLocale(input), expected);
    assert.equal(desktopLocale(input), expected);
  }
});
test("extension follows browser language automatically and preserves a manual preference", () => {
  const api = { i18n: { getUILanguage: () => "tr" } };
  assert.equal(preferredLocale(api), "tr-TR");
  assert.equal(preferredLocale(api, ""), "tr-TR");
  assert.equal(preferredLocale(api, "de-DE"), "de-DE");
  assert.equal(
    preferredLocale({ i18n: { getUILanguage: () => "ko" } }),
    "en-US",
  );
});
