import { getBrowserApi, isGecko, nativeAllowed } from "./browser-api.js";
const api = getBrowserApi();
import { NativeBridge, DurableOutbox } from "./bridge.js";
import { makeInput, mediaKind, MAX_CANDIDATES } from "./shared.js";
import { renewSource } from "./healing.js";

const bridge = new NativeBridge();
let healingTimer;
let healingBusy = false;
const refreshInFlight = new Set();
async function healPending() {
  if (healingBusy || !bridge.connected) return;
  healingBusy = true;
  try {
    const pending = await bridge.request("refresh_poll");
    if (!Array.isArray(pending)) return;
    await Promise.allSettled(
      pending
        .slice(0, 32)
        .filter((item) => !refreshInFlight.has(item.ticket))
        .map(async (item) => {
          refreshInFlight.add(item.ticket);
          try {
            await renewSource(item, {
              tabs: api.tabs,
              storage: api.storage.session,
              credentialsFor,
              bridge,
            });
          } finally {
            refreshInFlight.delete(item.ticket);
          }
        }),
    );
  } catch {
    /* The engine retains each ticket until its bounded deadline. */
  } finally {
    healingBusy = false;
  }
}
bridge.onConnected = () => {
  clearInterval(healingTimer);
  healingTimer = setInterval(() => {
    if (!bridge.connected) {
      clearInterval(healingTimer);
      return;
    }
    void healPending();
  }, 2000);
  healingTimer.unref?.();
};
async function credentialsFor(tabId, url) {
  await authGate;
  const downloadCredentials = (
    (await api.storage.session.get("auth:downloads"))["auth:downloads"] || []
  ).find((item) => item.url === url && Date.now() - item.seenAt < 120000);
  if (!Number.isInteger(tabId))
    return downloadCredentials ? [downloadCredentials.credentials] : [];
  const key = `auth:${tabId}`;
  const origin = new URL(url).origin;
  const origins = ((await api.storage.session.get(key))[key] || [])
    .filter((item) => Date.now() - item.seenAt < 30 * 60 * 1000)
    .sort((a, b) => Number(b.origin === origin) - Number(a.origin === origin))
    .slice(0, 8)
    .map(({ seenAt, ...credentials }) => credentials);
  return downloadCredentials
    ? [
        downloadCredentials.credentials,
        ...origins.filter((item) => item.origin !== origin),
      ].slice(0, 8)
    : origins;
}
const outbox = new DurableOutbox(
  bridge,
  async (entry, task, error) => {
    if (task && entry.downloadId !== undefined) {
      // The durable desktop proposal owns the transfer as soon as it is saved.
      // Approval, cancellation and network retries now belong only to Flux.
      try {
        await api.downloads.cancel(entry.downloadId);
        await api.downloads.erase({ id: entry.downloadId });
      } catch {
        /* Already completed or removed by the user. */
      }
    }
    const record = {
      id: entry.id,
      filename: entry.input.filename,
      ok: !!task,
      error: error?.message,
      time: Date.now(),
    };
    await api.storage.local.set({ lastTransfer: record });
    await api.action.setBadgeText({ text: error ? "!" : "" });
    await api.action.setBadgeBackgroundColor({ color: "#8b5cf6" });
  },
  async (entry) => {
    if (Number.isInteger(entry.context?.tabId)) {
      try {
        const tab = await api.tabs.get(entry.context.tabId);
        if (/^https?:\/\//i.test(tab.url))
          entry.input.browser_source = {
            page_url: tab.url,
            frame_url: entry.input.referer || null,
          };
      } catch {
        /* Closed pages cannot renew a source silently. */
      }
    }
    if (entry.input.protected_credentials) return entry.input;
    const credentials = await credentialsFor(
      entry.context?.tabId,
      entry.input.url,
    );
    if (!credentials.length) return entry.input;
    const protected_credentials = await bridge.request("protect", {
      credentials,
    });
    return { ...entry.input, protected_credentials };
  },
  "propose",
);
let mediaGate = Promise.resolve();
let authGate = Promise.resolve();
let interceptionGate = Promise.resolve();
let interceptEnabled = true;
let nativeEnabled = !isGecko(api);
const preferencesReady = api.storage.local
  .get("interceptDownloads")
  .then(async (value) => {
    nativeEnabled = await nativeAllowed(api);
    interceptEnabled = value.interceptDownloads ?? true;
  })
  .catch(() => {
    interceptEnabled = false;
  });
const earlyPauses = new Map();
function holdBrowserDownload(item) {
  if (earlyPauses.has(item.id)) return earlyPauses.get(item.id);
  if (earlyPauses.size >= 500) return Promise.resolve(false);
  const paused = (async () => {
    let held = false;
    try {
      await api.downloads.pause(item.id);
      held = true;
      await updateDrafts((drafts) => {
        if (!drafts.includes(item.id)) drafts.push(item.id);
      });
      return true;
    } catch {
      if (held) await api.downloads.resume(item.id).catch(() => {});
      return false;
    }
  })();
  earlyPauses.set(item.id, paused);
  return paused;
}
api.downloads.onCreated.addListener((item) => {
  if (!/^https?:\/\//i.test(item.finalUrl || item.url)) return;
  void preferencesReady.then(() => {
    if (!interceptEnabled || !nativeEnabled) return;
    if (!api.downloads.onDeterminingFilename) return interceptDownload(item);
    return holdBrowserDownload(item);
  });
});
function updateDrafts(callback) {
  const next = interceptionGate
    .catch(() => {})
    .then(async () => {
      const { browserDrafts = [] } =
        await api.storage.local.get("browserDrafts");
      await callback(browserDrafts);
      await api.storage.local.set({ browserDrafts });
    });
  interceptionGate = next;
  return next;
}
function recoverDrafts() {
  return updateDrafts(async (drafts) => {
    const { outbox = [] } = await api.storage.local.get("outbox");
    for (const id of drafts) {
      if (
        Number.isInteger(id) &&
        !outbox.some((entry) => entry.downloadId === id)
      ) {
        try {
          await api.downloads.resume(id);
        } catch {}
      }
    }
    drafts.length = 0;
  });
}
api.webRequest.onBeforeSendHeaders.addListener(
  (details) => {
    if (!nativeEnabled) return;
    const origin = new URL(details.url).origin;
    const value = (name) =>
      details.requestHeaders?.find(
        (header) => header.name.toLowerCase() === name,
      )?.value || null;
    const credentials = {
      origin,
      cookie: value("cookie"),
      authorization: value("authorization"),
      user_agent: value("user-agent"),
      seenAt: Date.now(),
    };

    authGate = authGate
      .catch(() => {})
      .then(async () => {
        if (details.tabId < 0) {
          const old =
            (await api.storage.session.get("auth:downloads"))[
              "auth:downloads"
            ] || [];
          const { seenAt, ...privateCredentials } = credentials;
          await api.storage.session.set({
            "auth:downloads": [
              {
                url: details.url,
                credentials: privateCredentials,
                referer: value("referer"),
                seenAt,
              },
              ...old.filter(
                (item) =>
                  item.url !== details.url && Date.now() - item.seenAt < 120000,
              ),
            ].slice(0, 60),
          });
          return;
        }
        const key = `auth:${details.tabId}`;
        const old = (await api.storage.session.get(key))[key] || [];
        const rest = old.filter((item) => item.origin !== origin);
        await api.storage.session.set({
          [key]: [credentials, ...rest].slice(0, 8),
        });
        const requestKey = `request:${details.tabId}`;
        const recent =
          (await api.storage.session.get(requestKey))[requestKey] || [];
        const referer = value("referer");
        await api.storage.session.set({
          [requestKey]: [
            { url: details.url, referer, seenAt: Date.now() },
            ...recent.filter((item) => item.url !== details.url),
          ].slice(0, 120),
        });
      });
  },
  { urls: ["http://*/*", "https://*/*"] },
  isGecko(api) ? ["requestHeaders"] : ["requestHeaders", "extraHeaders"],
);
function capture(tabId, url, contentType, referer, frameId) {
  const kind = mediaKind(url, contentType);
  if (
    tabId < 0 ||
    !kind ||
    (kind === "file" &&
      !/^video\//i.test(contentType) &&
      /\.(mp3|wav|ogg|aac|flac)(?:\?|$)/i.test(url))
  )
    return;
  mediaGate = mediaGate
    .catch(() => {})
    .then(async () => {
      const key = `media:${tabId}`;
      const data = await api.storage.session.get(key);
      const candidates = data[key] || [];
      const found = candidates.find((item) => item.url === url);
      const item = {
        url,
        kind,
        contentType,
        referer,
        frameId,
        seenAt: Date.now(),
      };
      if (found) Object.assign(found, item);
      else candidates.push(item);
      const limited = candidates.slice(-MAX_CANDIDATES);
      await api.storage.session.set({ [key]: limited });
      try {
        await api.tabs.sendMessage(
          tabId,
          {
            type: "candidates",
            candidates: limited.filter((item) => item.frameId === frameId),
          },
          { frameId },
        );
      } catch {
        /* No eligible content script in this tab. */
      }
    });
}
api.webRequest.onHeadersReceived.addListener(
  (details) => {
    if (!nativeEnabled) return;
    const type =
      details.responseHeaders?.find(
        (header) => header.name.toLowerCase() === "content-type",
      )?.value || "";
    capture(
      details.tabId,
      details.url,
      type,
      details.initiator,
      details.frameId,
    );
  },
  { urls: ["http://*/*", "https://*/*"] },
  ["responseHeaders"],
);
api.tabs.onRemoved.addListener((tabId) => {
  void api.storage.session.remove([
    `media:${tabId}`,
    `auth:${tabId}`,
    `links:${tabId}`,
    `request:${tabId}`,
  ]);
});
api.tabs.onUpdated.addListener((tabId, change) => {
  if (change.status === "loading")
    void api.storage.session.remove(`media:${tabId}`);
});
async function dictionary() {
  const { locale = "en-US" } = await api.storage.local.get("locale");
  const safeLocale = /^[a-z]{2,3}-[A-Z]{2}$/.test(locale) ? locale : "en-US";
  const response = await fetch(
    api.runtime.getURL(`locales/${safeLocale}.json`),
  );
  return response.ok
    ? response.json()
    : (await fetch(api.runtime.getURL("locales/en-US.json"))).json();
}
async function refreshMenus() {
  const words = await dictionary();
  await api.contextMenus.removeAll();
  api.contextMenus.create({
    id: "flux-link",
    title: words.downloadWith,
    contexts: ["link", "video", "audio"],
  });
  api.contextMenus.create({
    id: "flux-page",
    title: words.captureLinks,
    contexts: ["page"],
  });
}
api.storage.onChanged.addListener((changes, area) => {
  if (area === "local" && changes.nativeConsent && isGecko(api)) {
    nativeEnabled = changes.nativeConsent.newValue === true;
    if (!nativeEnabled) {
      bridge.disconnect();
      clearInterval(healingTimer);
      void Promise.allSettled([authGate, mediaGate]).then(async () => {
        if (!nativeEnabled) await api.storage.session.clear();
      });
      void outbox.mutate(async (entries) => {
        for (const entry of entries) {
          if (Number.isInteger(entry.downloadId))
            await api.downloads.resume(entry.downloadId).catch(() => {});
        }
        entries.length = 0;
      });
    }
  }
  if (area === "local" && changes.interceptDownloads)
    interceptEnabled = changes.interceptDownloads.newValue ?? true;
  if (area === "local" && changes.locale)
    void refreshMenus().catch(console.warn);
});
api.runtime.onInstalled.addListener(async () => {
  if (isGecko(api) && !(await nativeAllowed(api)))
    await api.runtime.openOptionsPage();
  const preferences = await api.storage.local.get([
    "interceptDownloads",
    "interceptionConfigured",
  ]);
  if (
    preferences.interceptDownloads === undefined ||
    !preferences.interceptionConfigured
  )
    await api.storage.local.set({ interceptDownloads: true });
  await refreshMenus();
  await api.alarms.create("flux-outbox", {
    periodInMinutes: isGecko(api) ? 1 : 0.5,
  });
});
api.contextMenus.onClicked.addListener(async (info, tab) => {
  try {
    if (info.menuItemId === "flux-link")
      await outbox.enqueue(
        makeInput(info.linkUrl || info.srcUrl, { referer: info.pageUrl }),
        undefined,
        { tabId: tab?.id },
      );
    else if (tab?.id !== undefined) {
      await api.tabs.sendMessage(tab.id, { type: "capture-links" });
      await api.action.openPopup();
    }
  } catch (error) {
    await api.storage.local.set({
      lastTransfer: { ok: false, error: error.message, time: Date.now() },
    });
  }
});
function interceptDownload(item) {
  return (async () => {
    await preferencesReady;
    if (
      !interceptEnabled ||
      !nativeEnabled ||
      !/^https?:\/\//i.test(item.finalUrl || item.url)
    )
      return;
    // Only intercept while the desktop is reachable. A browser download remains usable on failure.
    if (!(await holdBrowserDownload(item))) return;
    try {
      await bridge.request("list_page", { offset: 0, limit: 1 });
    } catch {
      await api.downloads.resume(item.id).catch(() => {});
      return;
    }
    const tabs = await api.tabs.query({});
    const tab =
      tabs.find((tab) => tab.url === item.referrer) ||
      tabs.find((tab) => tab.active);
    await authGate;
    const requestKey = `request:${tab?.id}`;
    const requests =
      (await api.storage.session.get(requestKey))[requestKey] || [];
    const sourceUrl = item.finalUrl || item.url;
    const captured =
      requests.find((request) => request.url === sourceUrl) ||
      (
        (await api.storage.session.get("auth:downloads"))["auth:downloads"] ||
        []
      ).find((item) => item.url === sourceUrl);
    try {
      await outbox.enqueue(
        makeInput(sourceUrl, {
          filename: item.filename,
          content_type: item.mime,
          referer: captured?.referer || item.referrer || tab?.url,
        }),
        item.id,
        { tabId: tab?.id },
      );
    } catch (error) {
      await api.downloads.resume(item.id).catch(() => {});
      throw error;
    }
  })()
    .catch(async (error) => {
      try {
        await api.downloads.resume(item.id);
      } catch {}
      console.warn("Flux interception failed", error);
    })
    .finally(async () => {
      earlyPauses.delete(item.id);
      try {
        await updateDrafts((drafts) => {
          const index = drafts.indexOf(item.id);
          if (index >= 0) drafts.splice(index, 1);
        });
      } catch (error) {
        console.warn("Flux browser recovery storage failed", error);
      }
    });
}
if (api.downloads.onDeterminingFilename) {
  api.downloads.onDeterminingFilename.addListener((item, suggest) => {
    // Chromium holds its filename gate until pause and durable handoff finish.
    void interceptDownload(item).finally(() => suggest());
    return true;
  });
}

api.runtime.onMessage.addListener((message, sender, respond) => {
  const handle = async () => {
    if (
      sender.id !== api.runtime.id ||
      (sender.tab && !/^https?:\/\//i.test(sender.url || ""))
    )
      throw new Error("Unauthorized extension sender");
    if (
      !message ||
      typeof message !== "object" ||
      typeof message.type !== "string"
    )
      throw new Error("Invalid extension message");
    if (message.type === "candidates") {
      const tabId = sender.tab?.id ?? message.tabId;
      if (!Number.isInteger(tabId)) throw new Error("Missing tab identifier");
      const key = `media:${tabId}`;
      const items = (await api.storage.session.get(key))[key] || [];
      return sender.tab
        ? items.filter((item) => item.frameId === sender.frameId)
        : items;
    }
    if (message.type === "dictionary") return dictionary();
    if (message.type === "resolve")
      return bridge.request("resolve", { url: message.url });
    if (message.type === "enqueue")
      return {
        id: await outbox.enqueue(
          makeInput(message.url, message.options),
          undefined,
          { tabId: sender.tab?.id ?? message.options?.tabId },
        ),
      };
    if (message.type === "inspect") {
      const credentials = await credentialsFor(
        sender.tab?.id ?? message.tabId,
        message.url,
      );
      const protected_credentials = credentials.length
        ? await bridge.request("protect", { credentials })
        : null;
      return bridge.request("inspect", {
        url: message.url,
        referer: message.referer || sender.tab?.url || null,
        protected_credentials,
      });
    }
    if (message.type === "status") {
      const [native, stored] = await Promise.allSettled([
        bridge.request("list_page", { offset: 0, limit: 20 }),
        api.storage.local.get([
          "outbox",
          "lastTransfer",
          "interceptDownloads",
          "locale",
        ]),
      ]);
      return {
        connected: native.status === "fulfilled",
        tasks: native.status === "fulfilled" ? native.value.tasks : [],
        ...(stored.status === "fulfilled" ? stored.value : {}),
      };
    }
    if (message.type === "page-links" && sender.tab?.id !== undefined) {
      const links = Array.isArray(message.links)
        ? message.links
            .filter(
              (url) => typeof url === "string" && /^https?:\/\//i.test(url),
            )
            .slice(0, 500)
        : [];
      await api.storage.session.set({ [`links:${sender.tab.id}`]: links });
      return { count: links.length };
    }
    if (message.type === "retry") {
      void outbox
        .mutate((entries) => {
          for (const item of entries) item.nextAt = 0;
        })
        .then(() => outbox.pump());
      return true;
    }
    throw new Error("Unsupported extension message");
  };
  handle().then(
    (result) => respond({ ok: true, result }),
    (error) => respond({ ok: false, error: error.message }),
  );
  return true;
});
api.alarms.onAlarm.addListener((alarm) => {
  if (alarm.name === "flux-outbox") {
    void outbox.pump();
    void healPending();
  }
});
api.runtime.onStartup.addListener(async () => {
  await api.alarms.create("flux-outbox", {
    periodInMinutes: isGecko(api) ? 1 : 0.5,
  });
  await recoverDrafts();
  void outbox.pump();
});
void recoverDrafts().catch(console.warn);
void outbox.pump();
