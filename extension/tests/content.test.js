import test from "node:test";
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { JSDOM } from "jsdom";

const script = await readFile(
  new URL("../content.js", import.meta.url),
  "utf8",
);
const words = JSON.parse(
  await readFile(new URL("../locales/tr-TR.json", import.meta.url), "utf8"),
);
async function fixture({
  url = "https://film.test/watch",
  src = "blob:https://film.test/playing",
  candidates = [],
  resolved,
  resolveWait,
  storage = {},
  popover = true,
  gecko = false,
  html = "<!doctype html><title>Actual Film</title><video></video>",
  inspected,
} = {}) {
  const dom = new JSDOM(
    html,
    { url, runScripts: "outside-only", pretendToBeVisual: true },
  );
  const { window } = dom,
    roots = [],
    calls = [],
    listeners = [],
    popovers = new WeakSet();
  const attach = window.Element.prototype.attachShadow,
    matches = window.Element.prototype.matches;
  window.Element.prototype.attachShadow = function (options) {
    const shadow = attach.call(this, options);
    roots.push(shadow);
    return shadow;
  };
  window.Element.prototype.matches = function (selector) {
    return selector === ":popover-open"
      ? popovers.has(this)
      : matches.call(this, selector);
  };
  if (popover)
    window.HTMLElement.prototype.showPopover = function () {
      popovers.add(this);
    };
  if (popover)
    window.HTMLElement.prototype.hidePopover = function () {
      popovers.delete(this);
    };
  window.HTMLVideoElement.prototype.getBoundingClientRect = () => ({
    width: 640,
    height: 360,
    top: 100,
    bottom: 460,
    left: 100,
    right: 740,
  });
  window.HTMLAudioElement.prototype.getBoundingClientRect = () => ({width:320,height:48,left:100,right:420,top:100,bottom:148});
  window.ResizeObserver = class {
    observe() {}
    disconnect() {}
  };
  window.document.querySelector("video,audio").src = src;
  window.chrome = {
    storage: {
      local: {
        async get(key) {
          return { [key]: structuredClone(storage[key]) };
        },
        async set(values) {
          Object.assign(storage, structuredClone(values));
        },
      },
    },
    i18n: { getMessage: (key) => words[key] || key },
    runtime: {
      onMessage: {
        addListener(fn) {
          listeners.push(fn);
        },
      },
      async sendMessage(message) {
        calls.push(message);
        let result;
        if (message.type === "dictionary") result = words;
        else if (message.type === "candidates") result = candidates;
        else if (message.type === "resolve") {
          if (resolveWait) await resolveWait;
          result = resolved;
        } else if (message.type === "inspect")
          result = inspected || {
            streams: [
              {
                codec_type: "video",
                codec_name: "h264",
                height: message.url.includes("1080") ? 1080 : 720,
                index: 0,
              },
              { codec_type: "audio", codec_name: "aac", index: 1 },
            ],
          };
        else if (message.type === "enqueue")
          result = { id: "pending-proposal" };
        return { ok: true, result };
      },
    },
  };
  if (gecko) {
    window.browser = window.chrome;
    delete window.chrome;
  }
  window.eval(script);
  await until(() => roots.length > 0);
  await new Promise((resolve) => setTimeout(resolve, 30));
  return { window, roots, calls, listeners, close: () => window.close() };
}
async function until(condition) {
  const deadline = Date.now() + 2000;
  while (!condition()) {
    assert.ok(Date.now() < deadline, "HUD timed out");
    await new Promise((resolve) => setTimeout(resolve, 10));
  }
}
test("audio DOM sources create a native-sized HUD and select real audio without a blob transfer",async()=>{
  const f=await fixture({html:'<!doctype html><title>Track</title><audio controls><source src="https://audio.test/track.m4a" type="audio/mp4"></audio>',
    src:"blob:https://film.test/audio",inspected:{streams:[{codec_type:"audio",codec_name:"aac",index:0}]}});
  try {
    assert.equal(f.roots[0].host.style.display,"block");
    f.roots[0].querySelector(".main").click();
    await until(()=>!f.roots[0].querySelector(".download").disabled);
    f.roots[0].querySelector(".download").click();
    await until(()=>f.calls.some((call)=>call.type==="enqueue"));
    const message=f.calls.find((call)=>call.type==="enqueue");
    assert.equal(message.url,"https://audio.test/track.m4a");
    assert.equal(message.options.media.container,"mp3");
    assert.equal(message.options.media.audio_stream,0);
    assert.equal(message.options.media.video_stream,null);
    assert.ok(f.calls.some((call)=>call.type==="dom-source" && call.url===message.url));
  } finally {f.close();}
});

test("provider HUD preserves 60fps, audio language selection and an explicitly selected subtitle",async()=>{
  const f=await fixture({url:"https://vimeo.com/123",src:"blob:https://vimeo.com/playing",resolved:{title:"Film",formats:[{
    id:"1080",height:1080,fps:60,url:"https://cdn.test/video",container:"mp4",audio_url:"https://cdn.test/default"}],
    audio_tracks:[{id:"fr",url:"https://cdn.test/fr",codec:"aac",language:"fr",bitrate:128}],
    subtitles:[{url:"https://cdn.test/en.vtt",index:null,language:"en",title:"English"}]}});
  try {
    const shadow=f.roots[0];shadow.querySelector(".main").click();
    await until(()=>!shadow.querySelector(".download").disabled);
    assert.match(shadow.querySelector("select").textContent,/1080p 60fps/);
    shadow.querySelector(`select[aria-label="${words.audioTrack}"]`).value="0";
    shadow.querySelector(`select[aria-label="${words.subtitleTrack}"]`).value="0";
    shadow.querySelector(".download").click();
    await until(()=>f.calls.some((call)=>call.type==="enqueue"));
    const media=f.calls.find((call)=>call.type==="enqueue").options.media;
    assert.equal(media.audio_url,"https://cdn.test/fr");
    assert.equal(media.provider.audio_format_id,"fr");
    assert.equal(media.subtitle_mode,"embed");
    assert.equal(media.subtitles[0].language,"en");
  } finally {f.close();}
});

test("same-frame source snapshot reads DOM and resource URLs without altering playback",async()=>{
  const f=await fixture({src:"https://film.test/actual.mp4"});
  try {
    let response;
    const result=f.listeners[0]({type:"source-snapshot"},{},(value)=>{response=value;});
    assert.equal(result,true);
    assert.ok(response.urls.includes("https://film.test/actual.mp4"));
    assert.equal(response.page_url,"https://film.test/watch");
    assert.equal(f.window.document.querySelector("video").currentTime,0);
  } finally {f.close();}
});

test("HUD close persists through layout changes and new players on the same page", async () => {
  const f = await fixture({ src: "https://film.test/actual.mp4" });
  try {
    const shadow = f.roots[0],
      host = shadow.host;
    assert.equal(host.style.display, "block");
    assert.equal(
      shadow.querySelector(".close").getAttribute("aria-label"),
      words.close,
    );
    shadow.querySelector(".close").click();
    f.window.dispatchEvent(new f.window.Event("resize"));
    f.window.document.body.append(f.window.document.createElement("video"));
    await until(() => f.roots.length === 2);
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.equal(f.window.sessionStorage.getItem("flux-dismissed:/watch"), "1");
    assert.ok(f.roots.every((root) => root.host.style.display === "none"));
  } finally {
    f.close();
  }
});
test("film choices use verified resolutions and exclude UI sounds and TS fragments", async () => {
  const f = await fixture({
    candidates: [
      {
        url: "https://film.test/no_input.mp3",
        kind: "file",
        contentType: "audio/mpeg",
        seenAt: 5,
      },
      {
        url: "https://film.test/fragment.ts",
        kind: "file",
        contentType: "video/mp2t",
        seenAt: 4,
      },
      { url: "https://film.test/1080.m3u8", kind: "hls", seenAt: 1 },
      { url: "https://film.test/720.m3u8", kind: "hls", seenAt: 2 },
    ],
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    await until(() => !shadow.querySelector(".download").disabled);
    assert.deepEqual(
      [...shadow.querySelector("select").options].map(
        (option) => option.textContent,
      ),
      ["1080p · MP4", "720p · MP4"],
    );
    assert.equal(shadow.querySelector(".title").textContent, "Actual Film");
    assert.ok(
      f.calls
        .filter((call) => call.type === "inspect")
        .every((call) => call.url.endsWith("m3u8")),
    );
    shadow.querySelector(".download").click();
    await until(() => f.calls.some((call) => call.type === "enqueue"));
    const capture = f.calls.find((call) => call.type === "enqueue");
    assert.equal(capture.url, "https://film.test/1080.m3u8");
    assert.equal(capture.options.filename, "Actual Film.mp4");
  } finally {
    f.close();
  }
});
test("YouTube HUD uses resolved video and its actual audio when MP3 is selected", async () => {
  const f = await fixture({
    url: "https://www.youtube.com/watch?v=fixture",
    candidates: [
      {
        url: "https://ui.test/success.mp3",
        kind: "file",
        contentType: "audio/mpeg",
      },
    ],
    resolved: {
      title: "Real YouTube Video",
      formats: [
        {
          id: "137",
          url: "https://video.test/content",
          audio_url: "https://audio.test/soundtrack",
          height: 1080,
          codec: "avc1",
          container: "mp4",
        },
      ],
    },
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    await until(() => !shadow.querySelector(".download").disabled);
    assert.equal(
      shadow.querySelector(".title").textContent,
      "Real YouTube Video",
    );
    shadow.querySelectorAll("select")[1].value = "mp3";
    shadow.querySelector(".download").click();
    await until(() => f.calls.some((call) => call.type === "enqueue"));
    const capture = f.calls.find((call) => call.type === "enqueue");
    assert.equal(capture.url, "https://video.test/content");
    assert.equal(
      capture.options.media.audio_url,
      "https://audio.test/soundtrack",
    );
    assert.equal(capture.options.media.container, "mp3");
    assert.equal(capture.options.filename, "Real YouTube Video.mp3");
    assert.deepEqual(
      JSON.parse(JSON.stringify(capture.options.media.provider)),
      {
        url: "https://www.youtube.com/watch?v=fixture",
        format_id: "137",
      audio_format_id:null,
        height: 1080,
      },
    );
    assert.ok(!f.calls.some((call) => call.type === "inspect"));
  } finally {
    f.close();
  }
});

const resolvedFixture = {
  title: "Test video",
  formats: [
    {
      id: "137",
      url: "https://video.test/content",
      height: 1080,
      container: "mp4",
    },
  ],
};
test("loading HUD never displays an empty quality selector", async () => {
  let release;
  const resolveWait = new Promise((r) => (release = r));
  const f = await fixture({
    url: "https://www.youtube.com/watch?v=loading",
    resolved: resolvedFixture,
    resolveWait,
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    assert.equal(shadow.querySelector("select").hidden, true);
    assert.equal(shadow.querySelector(".download").hidden, true);
    assert.equal(
      shadow.querySelector(".panel").getAttribute("aria-busy"),
      "true",
    );
    assert.equal(
      shadow.querySelector("[role=status]").textContent,
      words.inspecting,
    );
    release();
    await until(() => !shadow.querySelector(".download").disabled);
    assert.equal(shadow.querySelector("select").hidden, false);
    assert.equal(
      shadow.querySelector(".panel").getAttribute("aria-busy"),
      "false",
    );
  } finally {
    release();
    f.close();
  }
});
test("outside click closes only the panel and retains the video button", async () => {
  const f = await fixture({
    url: "https://www.youtube.com/watch?v=outside",
    resolved: resolvedFixture,
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    await until(() => !shadow.querySelector(".download").disabled);
    shadow.querySelector("select").click();
    assert.equal(shadow.querySelector(".panel").hidden, false);
    f.window.document.body.click();
    assert.equal(shadow.querySelector(".panel").hidden, true);
    assert.notEqual(
      f.window.document.querySelector("[data-flux-hud]").style.display,
      "none",
    );
    shadow.querySelector(".main").click();
    await until(
      () =>
        !shadow.querySelector(".panel").hidden &&
        !shadow.querySelector(".download").disabled,
    );
    assert.equal(f.calls.filter((c) => c.type === "resolve").length, 1);
    shadow.querySelector(".close").click();
    assert.equal(
      f.window.document.querySelector("[data-flux-hud]").style.display,
      "none",
    );
  } finally {
    f.close();
  }
});
test("closing the panel while inspection is pending cannot reopen it", async () => {
  let release;
  const resolveWait = new Promise((r) => (release = r));
  const f = await fixture({
    url: "https://www.youtube.com/watch?v=pending",
    resolved: resolvedFixture,
    resolveWait,
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    f.window.document.body.click();
    release();
    await new Promise((r) => setTimeout(r, 20));
    assert.equal(shadow.querySelector(".panel").hidden, true);
    assert.equal(
      shadow.querySelector(".main").getAttribute("aria-expanded"),
      "false",
    );
  } finally {
    release();
    f.close();
  }
});
test("video playback prepares formats before the user opens the HUD", async () => {
  const f = await fixture({
    url: "https://www.youtube.com/watch?v=prefetch",
    resolved: resolvedFixture,
  });
  try {
    const shadow = f.roots[0];
    f.window.document
      .querySelector("video")
      .dispatchEvent(new f.window.Event("play"));
    await until(() => f.calls.some((c) => c.type === "resolve"));
    shadow.querySelector(".main").click();
    await until(() => !shadow.querySelector(".download").disabled);
    assert.equal(f.calls.filter((c) => c.type === "resolve").length, 1);
  } finally {
    f.close();
  }
});
test("HUD position is domain-scoped, bounded and persists after native pointer dragging", async () => {
  const storage = { "hud-position:film.test": { x: 0, y: 0 } };
  const f = await fixture({
    storage,
    candidates: [{ url: "https://film.test/movie.m3u8", kind: "hls" }],
  });
  try {
    const host = f.window.document.querySelector("[data-flux-hud]");
    assert.match(host.style.transform, /translate3d\(100px,100px/);
    const grip = f.roots[0].querySelector(".move");
    for (const [type, x, y] of [
      ["pointerdown", 115, 115],
      ["pointermove", 900, 900],
      ["pointerup", 900, 900],
    ]) {
      const event = new f.window.MouseEvent(type, {
        bubbles: true,
        button: 0,
        clientX: x,
        clientY: y,
      });
      Object.defineProperty(event, "pointerId", { value: 7 });
      grip.dispatchEvent(event);
    }
    await until(() => storage["hud-position:film.test"].y === 1);
    assert.deepEqual(storage["hud-position:film.test"], {
      x: 1,
      y: 1,
      space: "viewport",
    });
    await new Promise((resolve) => setTimeout(resolve, 30));
    assert.match(host.style.transform, /translate3d\(816px,722px/);
  } finally {
    f.close();
  }
});
test("HUD keyboard movement resets to its default corner and invalid coordinates are ignored", async () => {
  const storage = { "hud-position:film.test": { x: NaN, y: Infinity } };
  const f = await fixture({
    storage,
    candidates: [{ url: "https://film.test/movie.m3u8", kind: "hls" }],
  });
  try {
    const grip = f.roots[0].querySelector(".move");
    grip.dispatchEvent(
      new f.window.KeyboardEvent("keydown", {
        key: "ArrowLeft",
        bubbles: true,
      }),
    );
    assert.equal(storage["hud-position:film.test"].space, "viewport");
    assert.equal(storage["hud-position:film.test"].x, (524 - 8) / 808);
    grip.dispatchEvent(
      new f.window.KeyboardEvent("keydown", { key: "Home", bubbles: true }),
    );
    assert.deepEqual(storage["hud-position:film.test"], {
      x: 1,
      y: 0,
      space: "video",
    });
    assert.equal(storage["hud-position:other.test"], undefined);
  } finally {
    f.close();
  }
});

test("HUD dragging preserves the grabbed point without jumping", async () => {
  const storage = {};
  const f = await fixture({
    storage,
    candidates: [{ url: "https://film.test/movie.m3u8", kind: "hls" }],
  });
  try {
    const host = f.window.document.querySelector("[data-flux-hud]");
    const grip = f.roots[0].querySelector(".move");
    assert.match(host.style.transform, /translate3d\(540px,100px/);
    for (const [type, x] of [
      ["pointerdown", 600],
      ["pointermove", 550],
      ["pointerup", 550],
    ]) {
      const event = new f.window.MouseEvent(type, {
        bubbles: true,
        button: 0,
        clientX: x,
        clientY: 115,
      });
      Object.defineProperty(event, "pointerId", { value: 9 });
      grip.dispatchEvent(event);
    }
    await until(() => !!storage["hud-position:film.test"]);
    assert.ok(Math.abs(storage["hud-position:film.test"].x - 482 / 808) < 1e-9);
    await until(
      () =>
        Math.abs(
          Number(/translate3d\(([-.\d]+)px/.exec(host.style.transform)?.[1]) -
            490,
        ) < 1e-6,
    );
  } finally {
    f.close();
  }
});

test("HUD can move outside the video to every viewport edge and retains its position on reload", async () => {
  const storage = {};
  const options = {
    storage,
    candidates: [{ url: "https://film.test/movie.m3u8", kind: "hls" }],
  };
  const f = await fixture(options);
  try {
    const grip = f.roots[0].querySelector(".move");
    for (const [type, x, y] of [
      ["pointerdown", 550, 110],
      ["pointermove", 10, 10],
      ["pointerup", 10, 10],
    ]) {
      const event = new f.window.MouseEvent(type, {
        button: 0,
        clientX: x,
        clientY: y,
      });
      Object.defineProperty(event, "pointerId", { value: 11 });
      grip.dispatchEvent(event);
    }
    await until(() =>
      /translate3d\(8px,8px/.test(f.roots[0].host.style.transform),
    );
    assert.deepEqual(storage["hud-position:film.test"], {
      x: 0,
      y: 0,
      space: "viewport",
    });
  } finally {
    f.close();
  }
  const restored = await fixture(options);
  try {
    assert.match(
      restored.roots[0].host.style.transform,
      /translate3d\(8px,8px/,
    );
  } finally {
    restored.close();
  }
});

test("a resolved paused YouTube Short shows a ready message and does not require playback", async () => {
  const f = await fixture({
    url: "https://www.youtube.com/shorts/fixture",
    resolved: resolvedFixture,
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    await until(() => !shadow.querySelector(".download").disabled);
    assert.equal(
      shadow.querySelector("[role=status]").textContent,
      words.mediaReady,
    );
    assert.notEqual(words.mediaReady, words.mediaHint);
    assert.equal(
      f.calls.find((call) => call.type === "resolve").url,
      "https://www.youtube.com/shorts/fixture",
    );
  } finally {
    f.close();
  }
});

test("navigation during inspection discards the previous video's formats", async () => {
  let release;
  const resolveWait = new Promise((r) => (release = r));
  const f = await fixture({
    url: "https://www.youtube.com/shorts/first",
    resolved: resolvedFixture,
    resolveWait,
  });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    f.window.history.pushState({}, "", "/shorts/second");
    release();
    await until(() => shadow.querySelector(".panel").hidden);
    shadow.querySelector(".download").click();
    assert.ok(!f.calls.some((call) => call.type === "enqueue"));
  } finally {
    release();
    f.close();
  }
});

test("replacing a video source invalidates its open download selection", async () => {
  const f = await fixture({ src: "https://film.test/first.mp4" });
  try {
    const shadow = f.roots[0];
    shadow.querySelector(".main").click();
    await until(() => !shadow.querySelector(".download").disabled);
    f.window.document.querySelector("video").src =
      "https://film.test/second.mp4";
    shadow.querySelector(".download").click();
    assert.equal(shadow.querySelector(".panel").hidden, true);
    assert.ok(!f.calls.some((call) => call.type === "enqueue"));
  } finally {
    f.close();
  }
});

test("Firefox without the Popover API closes its HUD without throwing", async () => {
  const f = await fixture({
    src: "https://film.test/movie.mp4",
    gecko: true,
    popover: false,
  });
  try {
    const errors = [];
    f.window.addEventListener("error", (event) => errors.push(event.error));
    f.roots[0].querySelector(".close").click();
    assert.equal(f.roots[0].host.style.display, "none");
    assert.deepEqual(errors, []);
  } finally {
    f.close();
  }
});

test("lost pointer capture stops dragging and persists the last viewport position", async () => {
  const storage = {};
  const f = await fixture({ storage, src: "https://film.test/movie.mp4" });
  try {
    const grip = f.roots[0].querySelector(".move");
    for (const [type, x] of [
      ["pointerdown", 550],
      ["pointermove", 350],
      ["lostpointercapture", 350],
      ["pointermove", 20],
    ]) {
      const event = new f.window.MouseEvent(type, {
        button: 0,
        clientX: x,
        clientY: 110,
      });
      Object.defineProperty(event, "pointerId", { value: 12 });
      grip.dispatchEvent(event);
    }
    assert.equal(storage["hud-position:film.test"].x, (340 - 8) / 808);
    assert.equal(grip.style.cursor, "grab");
  } finally {
    f.close();
  }
});

test("a freely placed HUD switches to the newly playing video without overlapping or downloading the previous source", async () => {
  const f = await fixture({ src: "https://film.test/first.mp4" });
  try {
    const second = f.window.document.createElement("video");
    second.src = "https://film.test/second.mp4";
    f.window.document.body.append(second);
    await until(() => f.roots.length === 2);
    f.roots[0]
      .querySelector(".move")
      .dispatchEvent(
        new f.window.KeyboardEvent("keydown", {
          key: "ArrowLeft",
          bubbles: true,
        }),
      );
    await until(() => f.roots[1].host.style.display === "none");
    second.dispatchEvent(new f.window.Event("play"));
    await until(
      () =>
        f.roots[0].host.style.display === "none" &&
        f.roots[1].host.style.display === "block",
    );
    f.roots[1].querySelector(".main").click();
    await until(() => !f.roots[1].querySelector(".download").disabled);
    f.roots[1].querySelector(".download").click();
    await until(() => f.calls.some((call) => call.type === "enqueue"));
    assert.equal(
      f.calls.find((call) => call.type === "enqueue").url,
      "https://film.test/second.mp4",
    );
  } finally {
    f.close();
  }
});
