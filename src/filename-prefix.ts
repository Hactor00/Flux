const historyKey = "flux.filename-prefix-history.v1";
const byteLength = (value: string) => new TextEncoder().encode(value).length;

function validName(value: string) {
  return (
    value.length > 0 &&
    byteLength(value) <= 200 &&
    !/[<>:"/\\|?*\x00-\x1f\x7f-\x9f]/.test(value) &&
    !/[. ]$/.test(value) &&
    !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(?:\.|$)/i.test(value)
  );
}

export function splitFilenamePrefix(prefix: string, filename: string) {
  const normalized = prefix.replace(/\\/g, "/");
  const parts = normalized.split("/");
  const suffix = parts.pop() || "";
  const name = suffix + filename;
  if (
    byteLength(normalized) > 512 ||
    parts.length > 16 ||
    !validName(filename) ||
    !validName(name) ||
    parts.some((part) => !validName(part))
  )
    throw new Error("Invalid filename prefix");
  return { prefix: normalized, folders: parts, filename: name };
}

export function filenamePrefixPreview(
  directory: string,
  prefix: string,
  filename: string,
) {
  const result = splitFilenamePrefix(prefix, filename);
  const separator = /^[a-z]:[\\/]|^\\\\/i.test(directory) ? "\\" : "/";
  return [directory.replace(/[\\/]+$/, ""), ...result.folders, result.filename]
    .filter(Boolean)
    .join(separator);
}

export function filenamePrefixHistory(
  storage: Pick<Storage, "getItem"> = localStorage,
): string[] {
  try {
    const saved: unknown = JSON.parse(storage.getItem(historyKey) || "[]");
    if (!Array.isArray(saved)) return [];
    const valid = saved
      .filter((value): value is string => {
        if (typeof value !== "string" || !value) return false;
        try {
          splitFilenamePrefix(value, "x");
          return true;
        } catch {
          return false;
        }
      })
      .map((value) => value.replace(/\\/g, "/"));
    return [...new Set(valid)].slice(0, 10);
  } catch {
    return [];
  }
}

export function rememberFilenamePrefix(
  prefix: string,
  storage: Pick<Storage, "getItem" | "setItem"> = localStorage,
) {
  const normalized = splitFilenamePrefix(prefix, "x").prefix;
  const previous = filenamePrefixHistory(storage);
  if (!normalized) return previous;
  const history = [
    normalized,
    ...previous.filter((value) => value !== normalized),
  ].slice(0, 10);
  try {
    storage.setItem(historyKey, JSON.stringify(history));
  } catch {}
  return history;
}
