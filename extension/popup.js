import { getBrowserApi } from "./browser-api.js";
const api = getBrowserApi();
import { localize, t } from "./i18n.js";
import { isProviderUrl, isCollectionUrl, mediaKind } from "./shared.js";
import { compareVersions } from "./release-update.js";
import { initializeTheme } from "./ui-theme.js";

let links = [];

let currentTab;
let desktopVersion;
let releaseVersion;
let legacyDesktop = false;
function empty(title, description) {
  const node = document.createElement("div");
  node.className = "empty";
  const heading = document.createElement("strong"),
    body = document.createElement("p");
  heading.textContent = t(title);
  body.textContent = t(description);
  node.append(heading, body);
  return node;
}
function selectTab(tab) {
  for (const button of document.querySelectorAll('[role="tab"]')) {
    const selected = button === tab;
    button.setAttribute("aria-selected", String(selected));
    button.tabIndex = selected ? 0 : -1;
    document.getElementById(button.getAttribute("aria-controls")).hidden =
      !selected;
  }
}
function updateSelection() {
  document.getElementById("download-links").disabled = !document.querySelector(
    "#links input:checked",
  );
}
function renderUpdate() {
  const extensionOld =
    compareVersions(releaseVersion, api.runtime.getManifest().version) === 1;
  const desktopOld =
    compareVersions(releaseVersion, desktopVersion) === 1 ||
    (legacyDesktop &&
      compareVersions(releaseVersion, "2.4.2") !== null &&
      compareVersions(releaseVersion, "2.4.2") >= 0);
  document.getElementById("update-notice").hidden =
    !extensionOld && !desktopOld;
  document.getElementById("update-title").textContent =
    `${t("updateAvailable")} ${releaseVersion || ""}`;
  document.getElementById("update-help").textContent = [
    extensionOld ? t("extensionUpdateHelp") : "",
    desktopOld ? t("desktopUpdateHelp") : "",
  ]
    .filter(Boolean)
    .join(" ");
}

async function send(message) {
  const response = await api.runtime.sendMessage(message);
  if (!response?.ok) throw new Error(response?.error || t("action_failed"));
  return response.result;
}

function item(name, detail, action) {
  const row = document.createElement("div");
  row.className = "item";

  const body = document.createElement("div");
  const label = document.createElement("strong");
  label.textContent = name;
  label.title = name;

  const small = document.createElement("small");
  small.textContent = detail;
  body.append(label, small);
  row.append(body);

  if (action) {
    const button = document.createElement("button");
    const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "none");
    svg.setAttribute("stroke", "currentColor");
    svg.setAttribute("stroke-width", "1.7");
    svg.setAttribute("aria-hidden", "true");
    const path = document.createElementNS(svg.namespaceURI, "path");
    path.setAttribute("d", "M12 3v12m-5-5 5 5 5-5M4 16v5h16v-5");
    svg.append(path);
    button.append(svg);
    button.setAttribute("aria-label", t("start"));
    button.addEventListener("click", async () => {
      button.disabled = true;
      try {
        await action();
        document.getElementById("message").textContent = t("sentToDesktop");
      } catch (error) {
        document.getElementById("message").textContent = error.message;
      } finally {
        button.disabled = false;
      }
    });
    row.append(button);
  }

  return row;
}

async function refresh() {
  const state = await send({ type: "status" });
  desktopVersion = state.desktop_version;
  legacyDesktop = state.connected && state.legacy_desktop;
  document.getElementById("desktop-version").textContent =
    state.connected && desktopVersion ? `Flux ${desktopVersion}` : "";
  document.getElementById("desktop-install-help").hidden = state.connected;
  renderUpdate();

  document.getElementById("dot").classList.toggle("online", state.connected);

  document.getElementById("connection").textContent = t(
    state.connected ? "engine_ready" : "desktopOffline",
  );

  const tasks = document.getElementById("tasks");
  tasks.replaceChildren();

  for (const task of state.tasks || [])
    tasks.append(
      item(
        task.destination.split(/[\\/]/).pop() || task.request.filename,
        t(task.status),
      ),
    );

  for (const entry of state.outbox || [])
    tasks.append(item(entry.input.filename, t("waitingForDesktop")));

  document.getElementById("tasks-count").textContent =
    (state.tasks?.length || 0) + (state.outbox?.length || 0);
  if (!tasks.childElementCount)
    tasks.append(empty("empty_title", "empty_body"));

  if (state.lastTransfer?.error)
    document.getElementById("message").textContent = state.lastTransfer.error;
}
async function renderMedia(candidates, tab, parent) {
  const sources = candidates.filter(
    (candidate) =>
      candidate.kind === "hls" ||
      candidate.kind === "dash" ||
      (candidate.source === "dom" &&
        /^audio\//i.test(candidate.contentType || "")) ||
      (/^video\//i.test(candidate.contentType || "") &&
        !/\.(ts|m4s)(?:\?|$)/i.test(candidate.url) &&
        !/video\/mp2t/i.test(candidate.contentType || "")),
  );
  sources.sort(
    (a, b) =>
      Number(b.kind !== "file") - Number(a.kind !== "file") ||
      b.seenAt - a.seenAt,
  );
  const results = await Promise.allSettled(
    sources.slice(0, 4).map(async (source) => {
      const info = await send({
        type: "inspect",
        url: source.url,
        tabId: tab.id,
        referer: tab.url,
      });
      const videos = info.streams.filter(
        (stream) => stream.codec_type === "video",
      );
      return (
        videos.length
          ? videos
          : info.streams
              .filter((stream) => stream.codec_type === "audio")
              .slice(0, 1)
      ).map((stream) => ({ source, stream }));
    }),
  );
  const variants = results
    .flatMap((result) => (result.status === "fulfilled" ? result.value : []))
    .sort((a, b) => (b.stream.height || 0) - (a.stream.height || 0));
  for (const { source, stream } of variants)
    parent.append(
      item(
        tab.title || t("downloadMedia"),
        [
          stream.height ? `${stream.height}p` : t("automatic"),
          stream.fps ? `${stream.fps.toFixed(2).replace(/\.00$/, "")} fps` : "",
          stream.codec_name,
        ]
          .filter(Boolean)
          .join(" · "),
        () =>
          send({
            type: "enqueue",
            url: source.url,
            options: {
              tabId: tab.id,
              referer: tab.url,
              filename:
                (tab.title || "Media") +
                (stream.codec_type === "audio" ? ".mp3" : ".mp4"),
              media: {
                container: stream.codec_type === "audio" ? "mp3" : "mp4",
                video_stream:
                  stream.codec_type === "video" ? stream.index : null,
                audio_stream:
                  stream.codec_type === "audio" ? stream.index : null,
              },
            },
          }),
      ),
    );
  const failure = results.find((result) => result.status === "rejected");
  if (!variants.length && failure)
    document.getElementById("message").textContent = failure.reason.message;
}

function renderLinks() {
  const pattern = document.getElementById("filter").value.trim();

  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");

  const matcher = new RegExp(escaped, "i");

  const parent = document.getElementById("links");
  parent.replaceChildren();

  for (const url of links.filter((url) => matcher.test(url))) {
    const row = item(url, new URL(url).hostname);
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked =
      !!mediaKind(url) ||
      isProviderUrl(url) ||
      /\.(zip|7z|rar|exe|msi|apk|iso|pdf|epub|dmg|deb|rpm)(?:[?#]|$)/i.test(
        url,
      );
    checkbox.value = url;
    checkbox.setAttribute("aria-label", url);
    checkbox.addEventListener("change", updateSelection);
    row.prepend(checkbox);
    parent.append(row);
  }
  if (!parent.childElementCount)
    parent.append(empty("no_results", "search_hint"));
  document.getElementById("links-count").textContent = links.length;
  updateSelection();
}

await localize();
await initializeTheme();
document.getElementById("extension-version").textContent =
  api.runtime.getManifest().version;
document.getElementById("extension-version").title = t("extension_version");
for (const tab of document.querySelectorAll('[role="tab"]')) {
  tab.addEventListener("click", () => selectTab(tab));
  tab.addEventListener("keydown", (event) => {
    const tabs = [...document.querySelectorAll('[role="tab"]')];
    const direction = document.documentElement.dir === "rtl" ? -1 : 1;
    const step =
      event.key === "ArrowRight"
        ? direction
        : event.key === "ArrowLeft"
          ? -direction
          : 0;
    const target =
      event.key === "Home"
        ? tabs[0]
        : event.key === "End"
          ? tabs.at(-1)
          : step
            ? tabs[(tabs.indexOf(tab) + step + tabs.length) % tabs.length]
            : null;
    if (target) {
      event.preventDefault();
      selectTab(target);
      target.focus();
    }
  });
}
const hudToggle = document.getElementById("show-media-buttons");
const preferences = await api.storage.local.get("showMediaButtons");
hudToggle.checked = preferences.showMediaButtons ?? true;
hudToggle.addEventListener("change", async () => {
  try {
    await api.storage.local.set({ showMediaButtons: hudToggle.checked });
  } catch (error) {
    hudToggle.checked = !hudToggle.checked;
    document.getElementById("message").textContent = error.message;
  }
});

document
  .getElementById("options")
  .addEventListener("click", () => api.runtime.openOptionsPage());

document.getElementById("retry").addEventListener("click", async (event) => {
  const button = event.currentTarget;
  button.disabled = true;
  try {
    await send({ type: "retry" });
    await refresh();
  } catch (error) {
    document.getElementById("message").textContent = error.message;
  } finally {
    button.disabled = false;
  }
});

document.getElementById("add").addEventListener("submit", async (event) => {
  event.preventDefault();
  const button = event.currentTarget.querySelector("button");
  button.disabled = true;

  try {
    await send({
      type: "enqueue",
      url: document.getElementById("url").value,
      options: { tabId: currentTab?.id, referer: currentTab?.url },
    });
    document.getElementById("url").value = "";
    document.getElementById("message").textContent = t("sentToDesktop");
    await refresh();
  } catch (error) {
    document.getElementById("message").textContent = error.message;
  } finally {
    button.disabled = false;
  }
});

document.getElementById("filter").addEventListener("input", renderLinks);

document
  .getElementById("download-links")
  .addEventListener("click", async (event) => {
    event.target.disabled = true;

    try {
      await send({
        type: "enqueue-batch",
        urls: [...document.querySelectorAll("#links input:checked")].map(
          (checkbox) => checkbox.value,
        ),
        tabId: currentTab?.id,
        referer: currentTab?.url,
      });
      document.getElementById("message").textContent = t("sentToDesktop");
    } catch (error) {
      document.getElementById("message").textContent = error.message;
    } finally {
      updateSelection();
    }
  });
document
  .getElementById("download-collection")
  .addEventListener("click", async (event) => {
    event.target.disabled = true;
    document.getElementById("message").textContent = t("inspecting");
    try {
      await send({
        type: "enqueue-collection",
        url: currentTab.url,
        tabId: currentTab.id,
      });
      document.getElementById("message").textContent = t("sentToDesktop");
      await refresh();
    } catch (error) {
      document.getElementById("message").textContent = error.message;
    } finally {
      event.target.disabled = false;
    }
  });

try {
  // Start independently of media inspection, which may need an offline desktop.
  void send({ type: "release-update" })
    .then((version) => {
      releaseVersion = version;
      renderUpdate();
    })
    .catch(() => {});
  await refresh();
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });

  currentTab = tab;
  if (tab?.url) {
    try {
      document.getElementById("current-site").textContent = new URL(
        tab.url,
      ).hostname.replace(/^www\./, "");
    } catch {}
  }

  if (tab) {
    document.getElementById("collection-section").hidden = !isCollectionUrl(
      tab.url,
    );
    try {
      await api.tabs.sendMessage(
        tab.id,
        { type: "capture-links" },
        { frameId: 0 },
      );
    } catch {}
    const stored = await api.storage.session.get(`links:${tab.id}`);
    links = stored[`links:${tab.id}`] || [];
    renderLinks();
    const candidates = await send({ type: "candidates", tabId: tab.id });
    const parent = document.getElementById("media");

    if (isProviderUrl(tab.url) && !isCollectionUrl(tab.url)) {
      parent.append(item(tab.title || t("downloadMedia"), t("inspecting")));

      const resolved = await send({
        type: "resolve",
        url: tab.url,
        tabId: tab.id,
      });
      parent.replaceChildren();

      for (const format of resolved.formats)
        parent.append(
          item(
            resolved.title,
            `${format.height || ""}p · ${format.container.toUpperCase()}${format.size ? ` · ${(format.size / 1048576).toFixed(1)} MB` : ""}`,
            () =>
              send({
                type: "enqueue",
                url: format.url,
                options: {
                  tabId: tab.id,
                  referer: tab.url,
                  filename: resolved.title + "." + format.container,
                  media: {
                    provider: {
                      url: tab.url,
                      format_id: format.id,
                      audio_format_id: null,
                      height: format.height,
                    },
                    container: format.container,
                    audio_url: format.audio_url,
                  },
                },
              }),
          ),
        );
    } else {
      await renderMedia(candidates, tab, parent);
      if (!parent.childElementCount)
        parent.append(item(t("noMedia"), t("mediaHint")));
    }

    renderLinks();
  }

  await refresh();
} catch (error) {
  document.getElementById("message").textContent = error.message;
} finally {
  const media = document.getElementById("media");
  document.getElementById("media-count").textContent =
    media.querySelectorAll(".item button").length;
  if (!media.querySelector("button"))
    media.replaceChildren(empty("noMedia", "mediaHint"));
  if (!document.getElementById("links").childElementCount) renderLinks();
}
