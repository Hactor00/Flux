(() => {
  const api = globalThis.browser ?? globalThis.chrome;
  const hasPopover = typeof HTMLElement.prototype.showPopover === "function";
  const overlays = new Map();
  const mediaResults = new Map();
  const blobHints = new Map();
  const reportedSources = new Set();
  const placementKey = `hud-position:${location.hostname}`;
  let placement = { x: 1, y: 0, space: "video" };
  let activeVideo;
  let showMediaButtons = true;
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
  const provider = () => {
    const host = location.hostname;
    if (/(^|\.)youtube(?:-nocookie)?\.com$/.test(host))
      return /^\/(watch|shorts\/|embed\/)/.test(location.pathname);
    return [
      "youtu.be",
      "vimeo.com",
      "dailymotion.com",
      "dai.ly",
      "twitch.tv",
      "soundcloud.com",
      "bandcamp.com",
      "facebook.com",
      "fb.watch",
      "instagram.com",
      "tiktok.com",
      "reddit.com",
      "redd.it",
      "twitter.com",
      "x.com",
      "bilibili.com",
      "ted.com",
      "archive.org",
    ].some((domain) => host === domain || host.endsWith(`.${domain}`));
  };
  const label = (item) =>
    [
      item.height
        ? `${item.height}p${item.fps >= 50 ? ` ${Math.round(item.fps)}fps` : ""}`
        : item.audio_only
          ? t("audioTrack")
          : item.kind?.toUpperCase(),
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
    const declared = [
      current,
      ...Array.from(video.querySelectorAll("source[src]")).map(
        (source) => source.src,
      ),
    ].filter(
      (url, index, values) =>
        /^https?:\/\//i.test(url) && values.indexOf(url) === index,
    );
    const audio = video.tagName === "AUDIO";
    if (declared.length)
      return declared.slice(0, 4).map((url) => ({
        url,
        kind: /\.m3u8(?:\?|$)/i.test(url)
          ? "hls"
          : /\.mpd(?:\?|$)/i.test(url)
            ? "dash"
            : "file",
        height: video.videoHeight || null,
        audio_only: audio,
      }));
    const hint = blobHints.get(current);
    const hintAudio =
      audio ||
      (/^audio\//i.test(hint?.mime || "") &&
        !/^video\//i.test(hint?.mime || ""));
    return candidates
      .filter(
        (item) =>
          item.kind === "hls" ||
          item.kind === "dash" ||
          ((hintAudio ? /^audio\//i : /^video\//i).test(
            item.contentType || "",
          ) &&
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
        try {
          const resolved = await send({ type: "resolve", url: sourcePage });
          if (!resolved?.formats?.length) throw new Error(t("noMedia"));
          return {
            title: resolved.title,
            pageUrl: sourcePage,
            provider: true,
            videoSource,
            choices: resolved.formats.map((format) => ({
              ...format,
              audio_tracks: resolved.audio_tracks || [],
              subtitles: resolved.subtitles || [],
              chapters: resolved.chapters || [],
            })),
          };
        } catch (error) {
          if (!inputs.length) throw error;
        }
      }
      if (!inputs.length) throw new Error(t("noMedia"));
      const domSubtitles = Array.from(
        video.querySelectorAll(
          'track[src][kind="subtitles"],track[src][kind="captions"]',
        ),
      )
        .filter(
          (track) =>
            /^https?:\/\//i.test(track.src) &&
            /^[a-zA-Z0-9-]{1,32}$/.test(track.srclang || ""),
        )
        .slice(0, 32)
        .map((track) => ({
          url: track.src,
          index: null,
          language: track.srclang,
          title: track.label || track.srclang,
        }));
      const inspected = await Promise.allSettled(
        inputs.map(async (selected) => {
          if (selected.kind === "hls" || selected.kind === "dash") {
            try {
              const resolved = await send({
                type: "resolve-manifest",
                url: selected.url,
              });
              return resolved.formats.map((format) => ({
                ...selected,
                ...format,
                native_manifest: true,
                audio_tracks: resolved.audio_tracks || [],
                subtitles: [
                  ...(resolved.subtitles || []),
                  ...domSubtitles,
                ].slice(0, 32),
                chapters: resolved.chapters || [],
              }));
            } catch {}
          }
          const info = await send({
            type: "inspect",
            url: selected.url,
            referer: sourcePage,
          });
          const videos = info.streams.filter(
            (stream) => stream.codec_type === "video",
          );
          const audioTracks = info.streams.filter(
            (stream) => stream.codec_type === "audio",
          );
          return (videos.length ? videos : audioTracks.slice(0, 1)).map(
            (stream) => ({
              ...selected,
              height: stream.height,
              fps: stream.fps || null,
              codec: stream.codec_name,
              video_stream: stream.codec_type === "video" ? stream.index : null,
              audio_stream: stream.codec_type === "audio" ? stream.index : null,
              audio_only: stream.codec_type === "audio",
              audio_tracks: audioTracks,
              container: stream.codec_type === "audio" ? "mp3" : "mp4",
              subtitles: [...(info.subtitles || []), ...domSubtitles].slice(
                0,
                32,
              ),
              chapters: info.chapters || [],
            }),
          );
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
  function modalContext() {
    const focused = document.activeElement?.closest?.(
      'dialog[open], [aria-modal="true"]',
    );
    if (focused && !focused.closest("[inert]")) return focused;
    return [...document.querySelectorAll('dialog[open], [aria-modal="true"]')]
      .filter((element) => {
        const style = getComputedStyle(element);
        return (
          !element.hidden &&
          element.getAttribute("aria-hidden") !== "true" &&
          !element.closest("[inert]") &&
          style.display !== "none" &&
          style.visibility !== "hidden"
        );
      })
      .at(-1);
  }
  function overlayRoot(video) {
    const fullscreen = document.fullscreenElement;
    const modal = video.closest(
      'dialog[open], [role="dialog"], [aria-modal="true"]',
    );
    if (modal && (!fullscreen || fullscreen.contains(modal))) return modal;
    if (fullscreen?.contains(video)) return fullscreen;
    return document.documentElement;
  }
  function placeHost(overlay, video) {
    const parent = overlayRoot(video);
    if (overlay.host.parentElement === parent) return;
    // A popover outside a modal remains inert even when painted above its backdrop.
    if (hasPopover && overlay.host.matches(":popover-open"))
      overlay.host.hidePopover();
    parent.append(overlay.host);
  }
  function schedule() {
    if (frame) return;
    frame = requestAnimationFrame(() => {
      frame = 0;
      if (pageUrl !== location.href) {
        pageUrl = location.href;
        mediaResults.clear();
        reportedSources.clear();
        for (const overlay of overlays.values()) overlay.closePanel();
      }
      const modal = modalContext();
      for (const [video, overlay] of overlays)
        if (video.isConnected) placeHost(overlay, video);
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
          (!document.fullscreenElement ||
            document.fullscreenElement.contains(video)) &&
          (!modal || modal.contains(video)) &&
          !video.closest("[inert]") &&
          !closed() &&
          box.width >= (video.tagName === "AUDIO" ? 160 : 200) &&
          box.height >= (video.tagName === "AUDIO" ? 24 : 100) &&
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
          showMediaButtons &&
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
        if (!hasPopover && visible) {
          // Legacy Gecko uses fixed positioning inside the player's transformed modal.
          overlay.host.style.transform = "none";
          const origin = overlay.host.getBoundingClientRect();
          const scaleX = origin.width / overlay.host.offsetWidth || 1;
          const scaleY = origin.height / overlay.host.offsetHeight || 1;
          overlay.host.style.transform = `translate3d(${(x - origin.left) / scaleX}px,${(y - origin.top) / scaleY}px,0)`;
        }
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
    reportSources(video);
    const host = document.createElement("div");
    host.setAttribute("data-flux-hud", "");
    if (hasPopover) host.popover = "manual";
    host.style.cssText =
      "all:initial;font:12px system-ui,sans-serif;color-scheme:dark;position:fixed;inset:auto;margin:0;border:0;padding:0;background:transparent;width:max-content;max-width:calc(100vw - 16px);height:auto;max-height:none;overflow:visible;left:0;top:0;z-index:2147483646;pointer-events:auto;display:none";
    const shadow = host.attachShadow({ mode: "closed" }),
      style = document.createElement("style");
    style.textContent = `
      :host{color-scheme:dark;font:12px system-ui,sans-serif}
      *,*:before,*:after{box-sizing:border-box}
      .bar{display:flex;align-items:center;gap:0;max-width:100%;height:30px;border:1px solid #ffffff29;border-radius:8px;background:#15161df5;box-shadow:0 3px 12px #0003}
      button,select{font:12px system-ui;border:1px solid #ffffff24;color:#f1eef7;background:#20222cf5;padding:7px 10px;cursor:pointer}
      button:hover{background:#34313f}button:disabled{opacity:.5;cursor:wait}
      button:focus-visible,select:focus-visible{outline:2px solid #b6a2f4;outline-offset:2px}
      .bar button{height:28px;border:0;background:transparent;display:inline-flex;align-items:center;justify-content:center;padding:0 8px}
      .bar button:hover{background:#ffffff12}.bar button:active{background:#ffffff20}
      .main{border-radius:0;min-width:0;gap:6px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;font-weight:500;font-size:11px}
      .main svg{width:13px;height:13px;flex:none;color:#c8b5ff}
      .move{border-radius:7px 0 0 7px;flex:none;width:25px;color:#a2a0b1;cursor:grab;touch-action:none}
      .move svg{width:12px;height:14px;pointer-events:none}
      .close{border-radius:0 7px 7px 0;flex:none;width:26px;color:#a2a0b1}
      .close svg{width:12px;height:12px;pointer-events:none}
      .panel{position:absolute;display:grid;gap:10px;background:#191b23fc;padding:14px;border:1px solid #ffffff24;border-radius:12px;box-shadow:0 12px 40px #0006;overflow-y:auto;overscroll-behavior:contain}
      .panel[hidden],.panel [hidden]{display:none}
      .panel[aria-busy=true] p[role=status]{min-height:38px;display:flex;align-items:center;gap:9px}
      .panel[aria-busy=true] p[role=status]:before{content:'';width:14px;height:14px;flex:none;border:2px solid #b6a2f433;border-top-color:#b6a2f4;border-radius:50%;animation:flux-spin .8s linear infinite}
      @keyframes flux-spin{to{transform:rotate(360deg)}}
      @media(prefers-reduced-motion:reduce){.panel[aria-busy=true] p[role=status]:before{animation:none}}
      select{width:100%;min-width:0;border-radius:7px;padding:8px}
      p{font-size:11px;line-height:1.5;margin:0;color:#c1c1cf;overflow-wrap:anywhere}
      .title{font-size:13px;color:#fff;font-weight:600;display:-webkit-box;-webkit-line-clamp:2;-webkit-box-orient:vertical;overflow:hidden}
      .download{border-radius:7px;background:#b6a2f4;color:#17121f;font-weight:600;min-height:34px}
    `;
    function icon(paths) {
      const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
      for (const [key, value] of Object.entries({
        viewBox: "0 0 24 24",
        fill: "none",
        stroke: "currentColor",
        "stroke-width": "1.6",
        "stroke-linecap": "round",
        "stroke-linejoin": "round",
        "aria-hidden": "true",
      }))
        svg.setAttribute(key, value);
      for (const d of paths) {
        const path = document.createElementNS(svg.namespaceURI, "path");
        path.setAttribute("d", d);
        svg.append(path);
      }
      return svg;
    }
    const bar = document.createElement("div");
    bar.className = "bar";
    const button = document.createElement("button");
    button.type = "button";
    button.className = "main";
    button.append(
      icon(["M12 3v12m-4-4 4 4 4-4", "M5 16v4h14v-4"]),
      document.createTextNode(t("downloadMedia")),
    );
    button.setAttribute("aria-label", t("downloadMedia"));
    const close = document.createElement("button");
    const grip = document.createElement("button");
    grip.type = "button";
    grip.className = "move";
    grip.append(
      icon(["M8 5h.01M16 5h.01M8 12h.01M16 12h.01M8 19h.01M16 19h.01"]),
    );
    grip.title = t("moveHud");
    grip.setAttribute("aria-label", t("moveHud"));
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
    close.append(icon(["m6 6 12 12M18 6 6 18"]));
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
    const audioChoice = document.createElement("select");
    audioChoice.setAttribute("aria-label", t("audioTrack"));
    const subtitleChoice = document.createElement("select");
    subtitleChoice.setAttribute("aria-label", t("subtitleTrack"));
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
      quality.hidden =
        output.hidden =
        download.hidden =
        audioChoice.hidden =
        subtitleChoice.hidden =
          true;
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
        refreshTracks();
        status.textContent = t("mediaReady");
        download.disabled = false;
      } catch (error) {
        if (current === generation) status.textContent = error.message;
      } finally {
        if (current === generation) panel.setAttribute("aria-busy", "false");
        schedule();
      }
    });
    function refreshTracks() {
      const selected = choices[Number(quality.value)];
      if (!selected) return;
      if (selected.audio_only) output.value = "mp3";
      for (const option of output.options)
        option.disabled = selected.audio_only && option.value !== "mp3";
      audioChoice.replaceChildren(
        new Option(t("originalAudio"), ""),
        ...(selected.audio_tracks || []).map(
          (track, index) =>
            new Option(
              [
                track.language,
                track.title,
                track.codec || track.codec_name,
                track.bitrate ? `${Math.round(track.bitrate)} kb/s` : "",
              ]
                .filter(Boolean)
                .join(" · ") || t("audioTrack"),
              String(index),
            ),
        ),
      );
      audioChoice.hidden =
        selected.audio_only || !selected.audio_tracks?.length;
      subtitleChoice.replaceChildren(
        new Option(t("noSubtitles"), ""),
        ...(selected.subtitles || []).map(
          (track, index) =>
            new Option(track.title || track.language, String(index)),
        ),
      );
      subtitleChoice.hidden =
        output.value === "mp3" || !selected.subtitles?.length;
      schedule();
    }
    quality.addEventListener("change", refreshTracks);
    output.addEventListener("change", refreshTracks);
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
        const audioTrack =
          audioChoice.value === ""
            ? null
            : selected.audio_tracks?.[Number(audioChoice.value)];
        const subtitles =
          container === "mp3" || subtitleChoice.value === ""
            ? []
            : [selected.subtitles[Number(subtitleChoice.value)]];
        const filename =
          mediaTitle.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").slice(0, 120) +
          "." +
          container;
        await send({
          type: "enqueue",
          url: selected.url,
          options: {
            nativeManifest: !!selected.native_manifest,
            filename,
            referer: sourceContext.pageUrl,
            media: {
              kind: selected.kind || "file",
              provider: sourceContext.provider
                ? {
                    url: sourceContext.pageUrl,
                    format_id: selected.id,
                    audio_format_id: audioTrack?.id || null,
                    height: selected.height || null,
                  }
                : null,
              container,
              audio_url:
                selected.kind === "dash" && selected.native_manifest
                  ? null
                  : audioTrack?.url || selected.audio_url || null,
              audio_stream: audioTrack?.index ?? selected.audio_stream ?? null,
              subtitles,
              subtitle_mode: subtitles.length ? "embed" : "none",
              chapters: selected.chapters || [],
              video_stream: selected.video_stream ?? null,
              manifest_video_id: selected.manifest_video_id || null,
              manifest_audio_id:
                audioTrack?.manifest_id || selected.manifest_audio_id || null,
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
      if (event.key === "Escape" && !panel.hidden) {
        event.preventDefault();
        event.stopPropagation();
        closePanel();
      }
    });
    for (const type of ["pointerdown", "pointerup", "click", "dblclick"])
      host.addEventListener(type, (event) => event.stopPropagation());
    panel.append(
      heading,
      quality,
      output,
      audioChoice,
      subtitleChoice,
      download,
      status,
    );
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
      video.addEventListener(event, () => {
        reportSources(video);
        schedule();
      });
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
      dimensions: { width: 180, height: 30 },
      cleanup: () => removeEventListener("blur", onBlur),
    });
    if (!video.paused) warm();
    schedule();
  }
  function reportSources(element) {
    const values = [
      {
        url: element.currentSrc || element.src,
        contentType:
          element.tagName === "AUDIO" ? "audio/unknown" : "video/unknown",
      },
      ...Array.from(element.querySelectorAll("source[src]")).map((source) => ({
        url: source.src,
        contentType:
          source.type ||
          (element.tagName === "AUDIO" ? "audio/unknown" : "video/unknown"),
      })),
    ];
    for (const value of values.slice(0, 8)) {
      if (
        !/^https?:\/\//i.test(value.url) ||
        value.url.length > 16384 ||
        reportedSources.has(value.url)
      )
        continue;
      reportedSources.add(value.url);
      if (reportedSources.size > 80)
        reportedSources.delete(reportedSources.values().next().value);
      void send({ type: "dom-source", ...value }).catch(() => {});
    }
  }
  function snapshotSources() {
    const urls = [];
    for (const element of document.querySelectorAll("video,audio")) {
      urls.push(element.currentSrc || element.src);
      for (const source of element.querySelectorAll("source[src]"))
        urls.push(source.src);
    }
    for (const entry of performance
      .getEntriesByType?.("resource")
      ?.slice(-200) || [])
      urls.push(entry.name);
    return [
      ...new Set(
        urls.filter(
          (url) =>
            typeof url === "string" &&
            /^https?:\/\//i.test(url) &&
            url.length <= 16384,
        ),
      ),
    ].slice(0, 200);
  }
  const observer = new MutationObserver((records) => {
    for (const record of records) {
      const parentMedia = record.target.closest?.("video,audio");
      if (parentMedia) reportSources(parentMedia);
      if (record.type === "attributes") {
        const media = record.target.closest?.("video,audio");
        if (media) reportSources(media);
      }
      for (const node of record.addedNodes) {
        if (
          node.nodeType !== Node.ELEMENT_NODE ||
          node.hasAttribute("data-flux-hud")
        )
          continue;
        if (node.matches("video,audio")) attach(node);
        for (const video of node.querySelectorAll("video,audio")) attach(video);
      }
    }
    schedule();
  });
  api.runtime.onMessage.addListener((message, _sender, respond) => {
    if (message.type === "candidates") {
      candidates = message.candidates;
      schedule();
    }
    if (message.type === "source-snapshot") {
      respond?.({ page_url: location.href, urls: snapshotSources() });
      return true;
    }
    if (message.type === "capture-links") {
      const links = [
        ...new Set(
          [...document.querySelectorAll("a[href]")]
            .map((anchor) => anchor.href)
            .filter((url) => /^https?:\/\//i.test(url)),
        ),
      ].slice(0, 500);
      void send({ type: "page-links", links }).then(
        () => respond?.({ links }),
        () => respond?.({ links: [] }),
      );
      return true;
    }
  });
  Promise.all([
    send({ type: "dictionary" }),
    send({ type: "candidates" }),
    placementReady,
    send({ type: "preferences" }).catch(() => ({ showMediaButtons: true })),
  ])
    .then(([words, items, , preferences]) => {
      dictionary = words;
      showMediaButtons = preferences?.showMediaButtons ?? true;
      api.storage?.onChanged?.addListener((changes, area) => {
        if (area === "local" && changes.showMediaButtons) {
          showMediaButtons = changes.showMediaButtons.newValue ?? true;
          if (showMediaButtons) {
            try {
              sessionStorage.removeItem(sessionKey());
            } catch {}
          }
          schedule();
        }
      });
      candidates = items;
      for (const video of document.querySelectorAll("video,audio"))
        attach(video);
      observer.observe(document.documentElement, {
        childList: true,
        subtree: true,
        attributes: true,
        attributeFilter: [
          "src",
          "type",
          "open",
          "inert",
          "aria-modal",
          "aria-hidden",
          "role",
        ],
      });
      addEventListener("message", (event) => {
        const hint = event.data;
        if (
          event.source === window &&
          event.origin === location.origin &&
          hint?.channel === "flux-media-hint-v2" &&
          hint.kind === "manifest" &&
          typeof hint.url === "string" &&
          /^https?:\/\//i.test(hint.url) &&
          hint.url.length <= 16384 &&
          typeof hint.text === "string" &&
          hint.text.length <= 128 * 1024
        ) {
          void send({
            type: "manifest-hint",
            url: hint.url,
            text: hint.text,
          }).catch(() => {});
          return;
        }
        if (
          event.source !== window ||
          event.origin !== location.origin ||
          !hint ||
          hint.channel !== "flux-media-hint-v2" ||
          !["blob", "media_source"].includes(hint.kind) ||
          typeof hint.url !== "string" ||
          !hint.url.startsWith(`blob:${location.origin}/`) ||
          hint.url.length > 16384 ||
          typeof hint.mime !== "string" ||
          hint.mime.length > 256
        )
          return;
        blobHints.set(hint.url, { mime: hint.mime, seenAt: Date.now() });
        if (blobHints.size > 80)
          blobHints.delete(blobHints.keys().next().value);
        schedule();
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
        for (const [video, overlay] of overlays) {
          placeHost(overlay, video);
          const { host } = overlay;
          if (hasPopover && host.matches(":popover-open")) host.hidePopover();
        }
        schedule();
      });
    })
    .catch(() => {});
})();
