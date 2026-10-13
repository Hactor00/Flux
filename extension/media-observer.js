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
