const VOLATILE = new Set([
  "signature",
  "sig",
  "token",
  "expires",
  "expiry",
  "policy",
  "key-pair-id",
  "__token__",
  "hdnts",
  "hdnea",
  "cf_clearance",
]);
export function resourceIdentity(value) {
  const url = new URL(value);
  if (!/^https?:$/.test(url.protocol) || url.username || url.password)
    throw new Error("Invalid refresh URL");
  const pairs = [...url.searchParams].filter(
    ([key]) =>
      !key.toLowerCase().startsWith("x-amz-") &&
      !VOLATILE.has(key.toLowerCase()),
  );
  pairs.sort(([a, av], [b, bv]) =>
    a < b ? -1 : a > b ? 1 : av < bv ? -1 : av > bv ? 1 : 0,
  );
  url.search = "";
  url.hash = "";
  for (const [key, value] of pairs) url.searchParams.append(key, value);
  return url.href;
}
export function matchResource(target, candidates) {
  const identity = resourceIdentity(target);
  return (
    candidates
      .filter((value) => {
        try {
          return value.length <= 16384 && resourceIdentity(value) === identity;
        } catch {
          return false;
        }
      })
      .find((value) => value !== target) ||
    candidates.find((value) => value === target)
  );
}
export function pageSources(html, pageUrl) {
  if (html.length > 4 * 1024 * 1024)
    throw new Error("Refresh page exceeds size limit");
  const unescaped = html
    .replace(/\\u0026/gi, "&")
    .replace(/\\\//g, "/")
    .replace(/&amp;/g, "&");
  const values = [...unescaped.matchAll(/https?:\/\/[^\s"'<>\\]+/g)]
    .slice(0, 2000)
    .map((match) => match[0]);
  for (const match of unescaped.matchAll(
    /(?:src|href)\s*=\s*["']([^"']+)["']/gi,
  )) {
    try {
      values.push(new URL(match[1], pageUrl).href);
    } catch {}
    if (values.length >= 2000) break;
  }
  return values;
}
async function boundedPage(response) {
  if (!response.ok) throw new Error("Refresh page is unavailable");
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > 4 * 1024 * 1024)
        throw new Error("Refresh page exceeds size limit");
      chunks.push(value);
    }
  } finally {
    await reader.cancel().catch(() => {});
  }
  const result = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) {
    result.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return new TextDecoder().decode(result);
}
export async function renewSource(
  ticket,
  { tabs, storage, fetchPage = fetch, credentialsFor, bridge },
) {
  const page = new URL(ticket.page_url);
  page.hash = "";
  const candidates = (await tabs.query({})).filter((tab) => {
    try {
      const url = new URL(tab.url);
      url.hash = "";
      return url.href === page.href;
    } catch {
      return false;
    }
  });
  if (!candidates.length)
    throw new Error("Original browser page is no longer open");
  const tab = candidates[0];
  const media = (await storage.get(`media:${tab.id}`))[`media:${tab.id}`] || [];
  let url = matchResource(
    ticket.url,
    media.sort((a, b) => b.seenAt - a.seenAt).map((item) => item.url),
  );
  if (!url || url === ticket.url) {
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 12000);
    try {
      const sourcePage = ticket.frame_url || page.href;
      const response = await fetchPage(sourcePage, {
        credentials: "include",
        cache: "no-store",
        signal: controller.signal,
      });
      const fresh = matchResource(
        ticket.url,
        pageSources(await boundedPage(response), sourcePage),
      );
      url = fresh || url || ticket.url;
    } finally {
      clearTimeout(timer);
    }
  }
  const credentials = await credentialsFor(tab.id, url);
  const protected_credentials = credentials.length
    ? await bridge.request("protect", { credentials })
    : null;
  await bridge.request("refresh_submit", {
    ticket: ticket.ticket,
    page_url: ticket.page_url,
    url,
    protected_credentials,
  });
  return url;
}
