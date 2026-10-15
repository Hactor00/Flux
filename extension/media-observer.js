(() => {
  const blobs = new WeakMap();
  let sent = 0,
    windowStart = 0;
  function publish(detail) {
    const now = Date.now();
    if (now - windowStart >= 1000) {
      windowStart = now;
      sent = 0;
    }
    if (++sent > 32) return;
    window.postMessage(
      { channel: "flux-media-hint-v2", ...detail },
      location.origin,
    );
  }
  let inspecting = 0;
  async function inspect(response) {
    if (
      !response?.body ||
      !/^https?:\/\//i.test(response.url) ||
      inspecting >= 2
    )
      return;
    const mime = response.headers.get("content-type") || "";
    const known =
      /\.m3u8(?:\?|$)|\.mpd(?:\?|$)/i.test(response.url) ||
      /mpegurl|dash\+xml/i.test(mime);
    if (
      !known &&
      !/text\/plain|application\/octet-stream|image\/jpeg/i.test(mime)
    )
      return;
    const size = Number(response.headers.get("content-length"));
    if (known && size > 128 * 1024) return;
    inspecting++;
    let reader, timer;
    try {
      const deadline = new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(new Error("Manifest inspection timed out")),
          3000,
        );
      });
      reader = response.clone().body.getReader();
      const decoder = new TextDecoder();
      let text = "",
        bytes = 0,
        identified = known;
      while (bytes <= 128 * 1024) {
        const next = await Promise.race([reader.read(), deadline]);
        if (next.done) break;
        bytes += next.value.byteLength;
        if (bytes > 128 * 1024) return;
        text += decoder.decode(next.value, { stream: true });
        if (!identified && text.trimStart()) {
          identified = /^\s*(?:#EXTM3U|(?:<\?xml[^>]*>\s*)?<MPD\b)/i.test(text);
          if (
            !identified &&
            (text.length >= 8192 || !text.trimStart().startsWith("<"))
          )
            return;
        }
      }
      text += decoder.decode();
      if (/^\s*(?:#EXTM3U|(?:<\?xml[^>]*>\s*)?<MPD\b)/i.test(text))
        publish({ kind: "manifest", url: response.url, mime, text });
    } catch {
    } finally {
      clearTimeout(timer);
      inspecting--;
      if (reader) void reader.cancel().catch(() => {});
    }
  }
  const fetch = window.fetch;
  if (typeof fetch === "function") {
    window.fetch = function (...args) {
      const pending = Reflect.apply(fetch, this, args);
      pending.then(
        (response) => {
          void inspect(response);
        },
        () => {},
      );
      return pending;
    };
  }
  if (typeof XMLHttpRequest === "function") {
    const open = XMLHttpRequest.prototype.open;
    XMLHttpRequest.prototype.open = function (...args) {
      this.addEventListener(
        "load",
        () => {
          if (
            (this.responseType === "" || this.responseType === "text") &&
            /^https?:\/\//i.test(this.responseURL)
          ) {
            const text = this.responseText;
            if (
              text.length <= 128 * 1024 &&
              /^\s*(?:#EXTM3U|(?:<\?xml[^>]*>\s*)?<MPD\b)/i.test(text)
            )
              publish({
                kind: "manifest",
                url: this.responseURL,
                mime: this.getResponseHeader("content-type") || "",
                text,
              });
          }
        },
        { once: true },
      );
      return Reflect.apply(open, this, args);
    };
  }
  const createObjectURL = URL.createObjectURL;
  if (typeof createObjectURL === "function") {
    URL.createObjectURL = function (source) {
      const url = Reflect.apply(createObjectURL, this, [source]);
      if (
        (typeof MediaSource === "function" && source instanceof MediaSource) ||
        (typeof Blob === "function" &&
          source instanceof Blob &&
          /^(video|audio)\//i.test(source.type))
      ) {
        blobs.set(source, url);
        publish({ url, mime: source.type || "", kind: "blob" });
      }
      return url;
    };
  }
  if (typeof MediaSource === "function") {
    const addSourceBuffer = MediaSource.prototype.addSourceBuffer;
    if (typeof addSourceBuffer === "function") {
      MediaSource.prototype.addSourceBuffer = function (mime) {
        const buffer = Reflect.apply(addSourceBuffer, this, [mime]);
        const url = blobs.get(this);
        if (url)
          publish({
            url,
            mime: String(mime).slice(0, 256),
            kind: "media_source",
          });
        return buffer;
      };
    }
  }
})();
