export const RELEASE_PAGE = "https://github.com/Hactor00/Flux/releases/latest";
const FEED = `${RELEASE_PAGE}/download/release-info.json`;
const INTERVAL = 6 * 60 * 60 * 1000;

export function compareVersions(left, right) {
  const parse = (value) =>
    typeof value === "string" &&
    /^\d+(?:\.\d+){0,3}$/.test(value) &&
    value.length <= 32
      ? value.split(".").map(Number)
      : null;
  const a = parse(left),
    b = parse(right);
  if (!a || !b || [...a, ...b].some((part) => part > 65535)) return null;
  for (let index = 0; index < Math.max(a.length, b.length); index++) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0);
    if (difference) return Math.sign(difference);
  }
  return 0;
}

export class ReleaseChecker {
  #pending;
  constructor(storage, fetcher = fetch, now = Date.now) {
    this.storage = storage;
    this.fetcher = fetcher;
    this.now = now;
  }
  async check() {
    if (this.#pending) return this.#pending;
    this.#pending = this.#check().finally(() => {
      this.#pending = null;
    });
    return this.#pending;
  }
  async #check() {
    const { releaseCheck: cached } = await this.storage.get("releaseCheck");
    if (
      cached &&
      Number.isFinite(cached.checkedAt) &&
      this.now() >= cached.checkedAt &&
      this.now() - cached.checkedAt < INTERVAL
    )
      return cached.version ?? null;
    try {
      const response = await this.fetcher(FEED, {
        credentials: "omit",
        cache: "no-store",
        signal: AbortSignal.timeout(10000),
      });
      if (!response.ok) throw Error("Release check unavailable");
      const text = await response.text();
      if (text.length > 65536)
        throw Error("Release metadata exceeds the size limit");
      const { version } = JSON.parse(text);
      if (compareVersions(version, "0") === null)
        throw Error("Invalid release version");
      await this.storage.set({
        releaseCheck: { version, checkedAt: this.now() },
      });
      return version;
    } catch {
      const version =
        compareVersions(cached?.version, "0") !== null ? cached.version : null;
      // Back off on offline/rate-limited responses; opening the popup stays fast.
      await this.storage.set({
        releaseCheck: {
          version,
          checkedAt: this.now() - INTERVAL + 15 * 60 * 1000,
        },
      });
      return version;
    }
  }
}
