(() => {
  const api = globalThis.browser ?? globalThis.chrome;
  const hasPopover = typeof HTMLElement.prototype.showPopover === "function";
  const overlays = new Map();
  const mediaResults = new Map();
  const placementKey = `hud-position:${location.hostname}`;
  let placement = { x: 1, y: 0, space: "video" };
  let activeVideo;
  let pageUrl = location.href;
  const margin = 8;
  const clamp = (value) => Math.max(0, Math.min(1, value));
  const placementReady = (
    api.storage?.local?.get?.(placementKey) || Promise.resolve({})
  )
    .then((data) => {
      const saved = data[placementKey];
      if (saved && Number.isFinite(saved.x) && Number.isFinite(saved.y))
        placement = {
          x: clamp(saved.x),
          y: clamp(saved.y),
          space: saved.space === "viewport" ? "viewport" : "video",
        };
    })
    .catch(() => {});
  function savePlacement() {
    void api.storage?.local
      ?.set?.({ [placementKey]: placement })
      ?.catch(() => {});
    schedule();
  }
  let candidates = [],
    dictionary = {},
    frame = 0;
  const t = (key) => dictionary[key] || api.i18n.getMessage(key) || key;
  const title = () =>
    (
      document.querySelector('meta[property="og:title"]')?.content ||
      document.title ||
      "Video"
    )
      .replace(/\s*[-|]\s*YouTube\s*$/, "")
      .trim();
  const provider = () =>
    /(^|\.)youtube(?:-nocookie)?\.com$/.test(location.hostname) &&
    /^\/(watch|shorts\/|embed\/)/.test(location.pathname);
  const label = (item) =>
    [
      item.height ? `${item.height}p` : item.kind?.toUpperCase(),
      item.container?.toUpperCase() || item.codec,
      item.size ? `${(item.size / 1048576).toFixed(1)} MB` : "",
    ]
      .filter(Boolean)
      .join(" · ");
  const sessionKey = () =>
    `flux-dismissed:${location.pathname}${location.search}`;
  const closed = () => {
    try {
      return sessionStorage.getItem(sessionKey()) === "1";
    } catch {
      return false;
    }
  };
  async function send(message) {
    const response = await api.runtime.sendMessage(message);
    if (!response?.ok) throw new Error(response?.error || t("action_failed"));
    return response.result;
  }
  function sources(video) {
    const current = video.currentSrc || video.src;
    if (/^https?:\/\//i.test(current))
      return [
        {
          url: current,
          kind: /\.m3u8(?:\?|$)/i.test(current)
            ? "hls"
            : /\.mpd(?:\?|$)/i.test(current)
              ? "dash"
              : "file",
          height: video.videoHeight || null,
        },
      ];
    return candidates
      .filter(
        (item) =>
          item.kind === "hls" ||
          item.kind === "dash" ||
          (/^video\//i.test(item.contentType || "") &&
            !/googlevideo\.com/.test(item.url) &&
            !/\.(ts|m4s)(?:\?|$)/i.test(item.url) &&
            !/video\/mp2t/i.test(item.contentType || "")),
      )
      .sort(
        (a, b) =>
          Number(b.kind !== "file") - Number(a.kind !== "file") ||
          b.seenAt - a.seenAt,
      )
      .slice(0, 4);
  }
  function prepare(video) {
    const inputs = sources(video);
    const sourcePage = location.href;
    const videoSource = video.currentSrc || video.src;
    const isProvider = provider();
    const key = isProvider
      ? JSON.stringify([sourcePage, videoSource])
      : JSON.stringify([
          sourcePage,
          videoSource,
          inputs.map((item) => item.url),
        ]);
    const cached = mediaResults.get(key);
    if (cached && cached.expires > Date.now()) return cached.promise;
    const promise = (async () => {
      if (isProvider) {
        const resolved = await send({ type: "resolve", url: sourcePage });
        if (!resolved?.formats?.length) throw new Error(t("noMedia"));
        return {
          title: resolved.title,
          pageUrl: sourcePage,
          provider: true,
          videoSource,
          choices: resolved.formats.map((format) => ({
            ...format,
            subtitles: resolved.subtitles || [],
            chapters: resolved.chapters || [],
          })),
        };
      }
      if (!inputs.length) throw new Error(t("noMedia"));
      const inspected = await Promise.allSettled(
        inputs.map(async (selected) => {
          const info = await send({
            type: "inspect",
            url: selected.url,
            referer: sourcePage,
          });
          return info.streams
            .filter((stream) => stream.codec_type === "video")
            .map((stream) => ({
              ...selected,
              height: stream.height,
              codec: stream.codec_name,
              video_stream: stream.index,
              container: "mp4",
              subtitles: info.subtitles || [],
              chapters: info.chapters || [],
            }));
        }),
      );
      const choices = inspected
        .flatMap((result) =>
          result.status === "fulfilled" ? result.value : [],
        )
        .sort((a, b) => (b.height || 0) - (a.height || 0));
      if (!choices.length)
        throw new Error(
          inspected.find((result) => result.status === "rejected")?.reason
            ?.message || t("noMedia"),
        );
      return {
        title: title(),
        choices,
        pageUrl: sourcePage,
        provider: false,
        videoSource,
      };
    })();
    mediaResults.set(key, { promise, expires: Date.now() + 120000 });
    if (mediaResults.size > 8)
      mediaResults.delete(mediaResults.keys().next().value);
    promise.catch(() => {
      if (mediaResults.get(key)?.promise === promise) mediaResults.delete(key);
    });
    return promise;
  }
  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (pageUrl !== location.href) {
        pageUrl = location.href;
        for (const overlay of overlays.values()) overlay.closePanel();
      }
      const positions = [...overlays].map(([video, overlay]) => ({
        video,
        overlay,
        box: video.getBoundingClientRect(),
        barBox: overlay.bar.getBoundingClientRect(),
        panelHeight: overlay.panel.scrollHeight,
      }));
      const eligible = positions.filter(
        ({ video, box }) =>
          video.isConnected &&
          !closed() &&
          box.width >= 200 &&
          box.height >= 100 &&
          box.bottom > 0 &&
          box.top < innerHeight &&
          box.right > 0 &&
          box.left < innerWidth &&
          (provider() || sources(video).length > 0),
      );
      if (
        placement.space === "viewport" &&
        !eligible.some(({ video }) => video === activeVideo)
      )
        activeVideo =
          eligible.find(({ video }) => !video.paused)?.video ||
          eligible[0]?.video;
      for (const { video, overlay, box, barBox, panelHeight } of positions) {
        if (!video.isConnected) {
          overlay.observer.disconnect();
          overlay.cleanup();
          overlay.host.remove();
          overlays.delete(video);
          continue;
        }
        const visible =
          eligible.some((item) => item.video === video) &&
          (placement.space !== "viewport" || video === activeVideo);
        if (!visible) overlay.closePanel();
        const videoSource = video.currentSrc || video.src;
        if (videoSource !== overlay.source) {
          overlay.closePanel();
          overlay.source = videoSource;
        }
        overlay.host.style.display = visible ? "block" : "none";
        if (hasPopover && visible && !overlay.host.matches(":popover-open"))
          overlay.host.showPopover();
        if (hasPopover && !visible && overlay.host.matches(":popover-open"))
          overlay.host.hidePopover();
        if (barBox.width > 0 && barBox.height > 0)
          overlay.dimensions = { width: barBox.width, height: barBox.height };
        const { width, height } = overlay.dimensions;
        const maximumX = Math.max(margin, innerWidth - width - margin);
        const maximumY = Math.max(margin, innerHeight - height - margin);
        const x = Math.max(
          margin,
          Math.min(
            maximumX,
            placement.space === "viewport"
              ? margin + placement.x * (maximumX - margin)
              : box.left + placement.x * Math.max(0, box.width - width),
          ),
        );
        const y = Math.max(
          margin,
          Math.min(
            maximumY,
            placement.space === "viewport"
              ? margin + placement.y * (maximumY - margin)
              : box.top + placement.y * Math.max(0, box.height - height),
          ),
        );
        overlay.host.style.transform = `translate3d(${x}px,${y}px,0)`;
        overlay.position = { x, y };
        const below = Math.max(0, innerHeight - y - height - margin - 6);
        const above = Math.max(0, y - margin - 6);
        const upwards = Math.max(panelHeight, 160) > below && above > below;
        const panelWidth = Math.min(320, Math.max(1, innerWidth - 2 * margin));
        overlay.panel.style.width = `${panelWidth}px`;
        overlay.panel.style.left = `${Math.max(margin, Math.min(x, innerWidth - panelWidth - margin)) - x}px`;
        overlay.panel.style.top = upwards ? "auto" : `${height + 6}px`;
        overlay.panel.style.bottom = upwards ? `${height + 6}px` : "auto";
        overlay.panel.style.maxHeight = `${Math.max(1, upwards ? above : below)}px`;
      }
    });
  }
  function attach(video) {
    if (overlays.has(video) || overlays.size >= 24) return;
    const host = document.createElement("div");
    host.setAttribute("data-flux-hud", "");
    if (hasPopover) host.popover = "manual";
    host.style.cssText =
      "all:initial;font:12px system-ui,sans-serif;color-scheme:dark;position:fixed;inset:auto;margin:0;border:0;padding:0;background:transparent;width:max-content;max-width:calc(100vw - 16px);height:auto;max-height:none;overflow:visible;left:0;top:0;z-index:2147483646;pointer-events:auto;display:none";
    const shadow = host.attachShadow({ mode: "closed" }),
      style = document.createElement("style");
    style.textContent =
      ":host{color-scheme:dark;font:12px system-ui,sans-serif}*,*:before,*:after{box-sizing:border-box}.bar{display:flex;gap:1px;max-width:100%}button,select{font:12px system-ui;border:1px solid #ffffff24;color:#f1eef7;background:#191b23f5;padding:9px 12px;cursor:pointer}button:hover{background:#454052}button:disabled{opacity:.5;cursor:wait}button:focus-visible,select:focus-visible{outline:2px solid #b6a2f4;outline-offset:2px}.main{border-radius:0;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis}.move{border-radius:7px 0 0 7px;flex:none}.close{border-radius:0 7px 7px 0;width:34px;padding:0;font-size:18px;flex:none}.panel{position:absolute;display:grid;gap:10px;background:#191b23fa;padding:14px;border:1px solid #ffffff24;border-radius:8px;box-shadow:0 10px 32px #0008;overflow-y:auto;overscroll-behavior:contain}.panel[hidden],.panel [hidden]{display:none}.panel[aria-busy=true] p[role=status]{min-height:38px;display:flex;align-items:center;gap:9px}.panel[aria-busy=true] p[role=status]:before{content:'';width:14px;height:14px;flex:none;border:2px solid #b6a2f433;border-top-color:#b6a2f4;border-radius:50%;animation:flux-spin .8s linear infinite}@keyframes flux-spin{to{transform:rotate(360deg)}}@media(prefers-reduced-motion:reduce){.panel[aria-busy=true] p[role=status]:before{animation:none}}select{width:100%;min-width:0;border-radius:4px;padding:7px}p{font-size:11px;line-height:1.5;margin:0;color:#c1c1cf;overflow-wrap:anywhere}.title{font-size:13px;color:#fff;font-weight:600;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}.download{border-radius:5px;background:#b6a2f4;color:#17121f;font-weight:600}";
    const bar = document.createElement("div");
    bar.className = "bar";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "main";
    button.textContent = `↓ ${t("downloadMedia")}`;
    const close = document.createElement("button");
    const grip = document.createElement("button");
    grip.type = "button";
    grip.className = "move";
    grip.textContent = "⠿";
    grip.title = t("moveHud");
    grip.setAttribute("aria-label", t("moveHud"));
    grip.style.cssText = "cursor:grab;touch-action:none;padding:8px";
    let dragging = false;
    let pointerId;
    let grabOffset = { x: 0, y: 0 };
    grip.addEventListener("pointerdown", (event) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      const position = overlays.get(video)?.position || { x: 0, y: 0 };
      activeVideo = video;
      grabOffset = {
        x: event.clientX - position.x,
        y: event.clientY - position.y,
      };
      closePanel();
      dragging = true;
      pointerId = event.pointerId;
      grip.setPointerCapture?.(event.pointerId);
      grip.style.cursor = "grabbing";
    });
    grip.addEventListener("pointermove", (event) => {
      if (!dragging || event.pointerId !== pointerId) return;
      const { width, height } = overlays.get(video).dimensions;
      placement = {
        space: "viewport",
        x: clamp(
          (event.clientX - margin - grabOffset.x) /
            Math.max(1, innerWidth - width - 2 * margin),
        ),
        y: clamp(
          (event.clientY - margin - grabOffset.y) /
            Math.max(1, innerHeight - height - 2 * margin),
        ),
      };
      schedule();
    });
    const finishDrag = (event) => {
      if (!dragging || (event && event.pointerId !== pointerId)) return;
      dragging = false;
      grip.style.cursor = "grab";
      if (grip.hasPointerCapture?.(pointerId))
        grip.releasePointerCapture(pointerId);
      savePlacement();
    };
    grip.addEventListener("pointerup", finishDrag);
    grip.addEventListener("pointercancel", finishDrag);
    grip.addEventListener("lostpointercapture", finishDrag);
    const onBlur = () => finishDrag();
    addEventListener("blur", onBlur);
    grip.addEventListener("keydown", (event) => {
      if (!event.key.startsWith("Arrow") && event.key !== "Home") return;
      event.preventDefault();
      event.stopPropagation();
      activeVideo = video;
      if (event.key === "Home") placement = { x: 1, y: 0, space: "video" };
      else {
        const { position, dimensions } = overlays.get(video);
        const step = event.shiftKey ? 48 : 16;
        placement = {
          space: "viewport",
          x: clamp(
            (position.x -
              margin +
              (event.key === "ArrowRight"
                ? step
                : event.key === "ArrowLeft"
                  ? -step
                  : 0)) /
              Math.max(1, innerWidth - dimensions.width - 2 * margin),
          ),
          y: clamp(
            (position.y -
              margin +
              (event.key === "ArrowDown"
                ? step
                : event.key === "ArrowUp"
                  ? -step
                  : 0)) /
              Math.max(1, innerHeight - dimensions.height - 2 * margin),
          ),
        };
      }
      savePlacement();
    });
    close.type = "button";
    close.className = "close";
    close.textContent = "×";
    close.setAttribute("aria-label", t("close"));
    close.addEventListener("click", () => {
      closePanel();
      try {
        sessionStorage.setItem(sessionKey(), "1");
      } catch {}
      if (hasPopover && host.matches(":popover-open")) host.hidePopover();
      host.style.display = "none";
    });
    bar.append(grip, button, close);
    const panel = document.createElement("div");
    panel.className = "panel";
    panel.hidden = true;
    const heading = document.createElement("p");
    heading.className = "title";
    const quality = document.createElement("select");
    quality.setAttribute("aria-label", t("quality"));
    const output = document.createElement("select");
    output.setAttribute("aria-label", t("container"));
    for (const format of ["mp4", "mkv", "mp3"])
      output.add(new Option(format.toUpperCase(), format));
    const status = document.createElement("p");
    status.setAttribute("role", "status");
    const download = document.createElement("button");
    download.type = "button";
    download.className = "download";
    download.textContent = t("start");
    let generation = 0,
      choices = [],
      mediaTitle = "",
      mediaContext;
    function closePanel() {
      const wasOpen = !panel.hidden;
      panel.hidden = true;
      button.setAttribute("aria-expanded", "false");
      generation++;
      choices = [];
      mediaContext = undefined;
      panel.setAttribute("aria-busy", "false");
      if (wasOpen) schedule();
    }
    const warm = () => {
      void prepare(video).catch(() => {});
    };
    button.setAttribute("aria-expanded", "false");
    button.addEventListener("pointerenter", warm);
    button.addEventListener("focus", warm);
    button.addEventListener("click", async () => {
      if (!panel.hidden) {
        closePanel();
        return;
      }
      for (const overlay of overlays.values()) overlay.closePanel();
      panel.hidden = false;
      button.setAttribute("aria-expanded", "true");
      panel.setAttribute("aria-busy", "true");
      const current = ++generation;
      const sourcePage = location.href;
      activeVideo = video;
      mediaTitle = title();
      heading.textContent = mediaTitle;
      quality.replaceChildren();
      quality.hidden = output.hidden = download.hidden = true;
      download.disabled = true;
      status.textContent = t("inspecting");
      schedule();
      try {
        const ready = await prepare(video);
        if (current !== generation) return;
        if (
          sourcePage !== location.href ||
          ready.videoSource !== (video.currentSrc || video.src)
        ) {
          closePanel();
          return;
        }
        mediaContext = ready;
        mediaTitle = ready.title;
        heading.textContent = mediaTitle;
        choices = ready.choices;
        quality.replaceChildren(
          ...choices.map(
            (item, index) =>
              new Option(label(item) || t("automatic"), String(index)),
          ),
        );
        quality.hidden = output.hidden = download.hidden = false;
        status.textContent = t("mediaReady");
        download.disabled = false;
      } catch (error) {
        if (current === generation) status.textContent = error.message;
      } finally {
        if (current === generation) panel.setAttribute("aria-busy", "false");
        schedule();
      }
    });
    download.addEventListener("click", async () => {
      const selected = choices[Number(quality.value)];
      if (!selected || !mediaContext) return;
      if (
        mediaContext.pageUrl !== location.href ||
        mediaContext.videoSource !== (video.currentSrc || video.src)
      ) {
        closePanel();
        return;
      }
      const sourceContext = mediaContext;
      const current = generation;
      download.disabled = true;
      try {
        const container = output.value;
        const filename =
          mediaTitle.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").slice(0, 120) +
          "." +
          container;
        await send({
          type: "enqueue",
          url: selected.url,
          options: {
            filename,
            referer: sourceContext.pageUrl,
            media: {
              kind: selected.kind || "file",
              provider: sourceContext.provider
                ? {
                    url: sourceContext.pageUrl,
                    format_id: selected.id,
                    height: selected.height || null,
                  }
                : null,
              container,
              audio_url: selected.audio_url || null,
              subtitles: selected.subtitles || [],
              chapters: selected.chapters || [],
              video_stream: selected.video_stream ?? null,
            },
          },
        });
        if (current === generation) {
          status.textContent = t("sentToDesktop");
          closePanel();
        }
      } catch (error) {
        if (current === generation) status.textContent = error.message;
      } finally {
        if (current === generation) download.disabled = false;
        schedule();
      }
    });
    host.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        closePanel();
      }
    });
    panel.append(heading, quality, output, download, status);
    try {
      const sheet = new CSSStyleSheet();
      sheet.replaceSync(style.textContent);
      shadow.adoptedStyleSheets = [sheet];
      shadow.append(bar, panel);
    } catch {
      shadow.append(style, bar, panel);
    }
    document.documentElement.append(host);
    const observer = new ResizeObserver(schedule);
    observer.observe(video);
    observer.observe(bar);
    observer.observe(panel);
    for (const event of ["loadedmetadata", "play", "emptied"])
      video.addEventListener(event, schedule);
    for (const event of ["loadedmetadata", "play"])
      video.addEventListener(event, warm);
    video.addEventListener("play", () => {
      activeVideo = video;
      schedule();
    });
    overlays.set(video, {
      host,
      observer,
      closePanel,
      panel,
      bar,
      source: video.currentSrc || video.src,
      dimensions: { width: 200, height: 38 },
      cleanup: () => removeEventListener("blur", onBlur),
    });
    if (!video.paused) warm();
    schedule();
  }
  const observer = new MutationObserver((records) => {
    for (const record of records)
      for (const node of record.addedNodes) {
        if (
          node.nodeType !== Node.ELEMENT_NODE ||
          node.hasAttribute("data-flux-hud")
        )
          continue;
        if (node.matches("video")) attach(node);
        for (const video of node.querySelectorAll("video")) attach(video);
      }
    schedule();
  });
  api.runtime.onMessage.addListener((message) => {
    if (message.type === "candidates") {
      candidates = message.candidates;
      schedule();
    }
    if (message.type === "capture-links") {
      const links = [
        ...new Set(
          [...document.querySelectorAll("a[href]")]
            .map((anchor) => anchor.href)
            .filter((url) => /^https?:\/\//i.test(url)),
        ),
      ].slice(0, 500);
      void send({ type: "page-links", links });
    }
  });
  Promise.all([
    send({ type: "dictionary" }),
    send({ type: "candidates" }),
    placementReady,
  ])
    .then(([words, items]) => {
      dictionary = words;
      candidates = items;
      for (const video of document.querySelectorAll("video")) attach(video);
      observer.observe(document.documentElement, {
        childList: true,
        subtree: true,
      });
      addEventListener("scroll", schedule, { passive: true, capture: true });
      addEventListener("resize", schedule, { passive: true });
      addEventListener("popstate", schedule);
      addEventListener("hashchange", schedule);
      const outside = (event) => {
        for (const overlay of overlays.values())
          if (!event.composedPath().includes(overlay.host))
            overlay.closePanel();
      };
      document.addEventListener("pointerdown", outside, true);
      document.addEventListener("click", outside, true);
      addEventListener("fullscreenchange", () => {
        for (const { host } of overlays.values()) {
          if (!hasPopover)
            (document.fullscreenElement || document.documentElement).append(
              host,
            );
          if (hasPopover && host.matches(":popover-open")) host.hidePopover();
        }
        schedule();
      });
    })
    .catch(() => {});
})();
