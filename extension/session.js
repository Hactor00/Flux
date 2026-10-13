import { nativeAllowed } from "./browser-api.js";

function originOf(value) {
  try {
    const url = new URL(value);
    if (!/^https?:$/.test(url.protocol) || url.username || url.password)
      return null;
    return url.origin;
  } catch {
    return null;
  }
}

export async function refreshCredentials(
  api,
  tabId,
  targetUrl,
  captured,
  candidates = [],
  frameId = 0,
) {
  if (!(await nativeAllowed(api))) return [];
  const targetOrigin = originOf(targetUrl);
  if (!targetOrigin) throw new Error("Invalid credential origin");
  let tab;
  if (Number.isInteger(tabId)) {
    try {
      tab = await api.tabs.get(tabId);
    } catch {}
  }
  const allowed = new Set([targetOrigin, originOf(tab?.url)]);
  for (const source of candidates.slice(-80)) {
    allowed.add(originOf(source.url));
    allowed.add(originOf(source.referer));
  }
  allowed.delete(null);
  const scoped = captured
    .filter((item) => allowed.has(item.origin))
    .slice(0, 8)
    .map((item) => ({ ...item }));
  let credentials = scoped.find((item) => item.origin === targetOrigin);
  if (!credentials) {
    credentials = {
      origin: targetOrigin,
      cookie: null,
      authorization: null,
      user_agent: null,
    };
    scoped.unshift(credentials);
  }
  if (!api.cookies?.getAll)
    return scoped
      .filter((item) => item.cookie || item.authorization || item.user_agent)
      .slice(0, 8);
  try {
    const filters = { url: targetUrl };
    if (tab?.cookieStoreId) filters.storeId = tab.cookieStoreId;
    let partitionKey;
    if (Number.isInteger(tabId) && api.cookies.getPartitionKey) {
      try {
        const result = await api.cookies.getPartitionKey({ tabId, frameId });
        partitionKey = result.partitionKey;
      } catch {}
    }
    const regular = await api.cookies.getAll(filters);
    let partitioned = [];
    if (partitionKey?.topLevelSite) {
      partitioned = await api.cookies.getAll({ ...filters, partitionKey });
    }
    const cookies = [
      ...regular.filter((cookie) => !cookie.partitionKey),
      ...partitioned,
    ]
      .filter(
        (cookie) =>
          typeof cookie.name === "string" &&
          typeof cookie.value === "string" &&
          !/[;\r\n\0]/.test(cookie.name) &&
          !/[\r\n\0]/.test(cookie.value) &&
          (!cookie.expirationDate || cookie.expirationDate * 1000 > Date.now()),
      )
      .sort((a, b) => (b.path?.length || 0) - (a.path?.length || 0));
    const unique = new Set();
    let size = 0;
    const selected = [];
    for (const cookie of cookies.slice(0, 300)) {
      const identity = `${cookie.name}\0${cookie.domain}\0${cookie.path}\0${cookie.partitionKey?.topLevelSite || ""}`;
      const pair = `${cookie.name}=${cookie.value}`;
      if (unique.has(identity) || size + pair.length > 32768) continue;
      unique.add(identity);
      size += pair.length + 2;
      selected.push(pair);
    }
    credentials.cookie = selected.join("; ") || null;
  } catch {
    // Recent request headers remain valid when a cookie store cannot be queried.
  }
  return scoped
    .filter((item) => item.cookie || item.authorization || item.user_agent)
    .slice(0, 8);
}
