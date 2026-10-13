/** Both supported engines provide Promise-based WebExtension APIs. */
export function getBrowserApi() {
  const api = globalThis.browser ?? globalThis.chrome;
  if (!api) throw new Error("WebExtension runtime is unavailable");
  return api;
}

export function isGecko(api = getBrowserApi()) {
  return typeof api.runtime?.getBrowserInfo === "function";
}

export async function nativeAllowed(api = getBrowserApi()) {
  if (!isGecko(api)) return true;
  // Read the persisted flag at each native boundary; storage events may arrive later.
  return (await api.storage.local.get("nativeConsent")).nativeConsent === true;
}
