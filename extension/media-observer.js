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
  function inspect(url, mime, value) {
    if (!/^https?:\/\//i.test(url)) return;
    if (typeof value !== "string") {
      if (!value || value.byteLength > 128 * 1024) return;
      const bytes = new Uint8Array(value);
      const start = new TextDecoder().decode(bytes.subarray(0, 8192));
      if (!/^\s*(?:#EXTM3U|(?:<\?xml[^>]*>\s*)?<MPD\b)/i.test(start)) return;
      value = new TextDecoder().decode(bytes);
    }
    if (
      value.length <= 128 * 1024 &&
      /^\s*(?:#EXTM3U|(?:<\?xml[^>]*>\s*)?<MPD\b)/i.test(value)
    )
      publish({ kind: "manifest", url, mime, text: value });
  }
  if (typeof Response === "function") {
    for (const method of ["text", "arrayBuffer"]) {
      const read = Response.prototype[method];
      if (typeof read !== "function") continue;
      Response.prototype[method] = function (...args) {
        const pending = Reflect.apply(read, this, args);
        if (/^https?:\/\//i.test(this.url)) {
          const url = this.url;
          const mime = this.headers.get("content-type") || "";
          void pending
            .then(
              (value) => inspect(url, mime, value),
              () => {},
            )
            .catch(() => {});
        }
        return pending;
      };
    }
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
            inspect(
              this.responseURL,
              this.getResponseHeader("content-type") || "",
              this.responseText,
            );
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
