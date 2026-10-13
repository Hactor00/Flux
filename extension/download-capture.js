export function attachmentFilename(headers) {
  const disposition =
    headers.find(
      (header) => header.name.toLowerCase() === "content-disposition",
    )?.value || "";
  const encoded = disposition.match(/filename\*\s*=\s*([^;]+)/i)?.[1].trim();
  if (encoded) {
    const value = encoded.replace(/^"|"$/g, "");
    const parts = value.match(/^utf-8'[^']*'(.*)$/i);
    if (parts) {
      try {
        return decodeURIComponent(parts[1]);
      } catch {}
    }
  }
  return disposition
    .match(/filename\s*=\s*(?:"([^"]*)"|([^;]*))/i)
    ?.slice(1)
    .find(Boolean)
    ?.trim();
}

export function isAttachment(details) {
  if (details.method !== "GET" || !/^https?:\/\//i.test(details.url))
    return false;
  const headers = details.responseHeaders || [];
  const disposition =
    headers.find(
      (header) => header.name.toLowerCase() === "content-disposition",
    )?.value || "";
  return /^attachment(?:\s*;|\s*$)/i.test(disposition);
}
