import { getBrowserApi } from "./browser-api.js";
const api = getBrowserApi();
import { localize, t } from "./i18n.js";
import { isProviderUrl, isCollectionUrl, mediaKind } from "./shared.js";

let links = [];

let currentTab;

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
    button.textContent = "↓";
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

  if (!tasks.childElementCount)
    tasks.append(item(t("empty_title"), t("empty_body")));

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
    row.prepend(checkbox);
    parent.append(row);
  }
}

await localize();

document
  .getElementById("options")
  .addEventListener("click", () => api.runtime.openOptionsPage());

document.getElementById("retry").addEventListener("click", async () => {
  await send({ type: "retry" });
  await refresh();
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
      event.target.disabled = false;
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
  const [tab] = await api.tabs.query({ active: true, currentWindow: true });

  currentTab = tab;

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

    const stored = await api.storage.session.get(`links:${tab.id}`);
    links = stored[`links:${tab.id}`] || [];

    document.getElementById("links-section").hidden = !links.length;
    renderLinks();
  }

  await refresh();
} catch (error) {
  document.getElementById("message").textContent = error.message;
}
