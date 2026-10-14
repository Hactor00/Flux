import { useEffect, useRef, useState, type FormEvent } from "react";
import { isTauri } from "@tauri-apps/api/core";
import * as Dialog from "@radix-ui/react-dialog";
import * as Tabs from "@radix-ui/react-tabs";
import { Download, X, FolderOpen } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import {
  api,
  desktop,
  type Input,
  type MediaInfo,
  type Task,
  type ResolvedMedia,
  type CollectionEntry,
  type TorrentOptions,
} from "../api";
import {
  collectionInputs,
  sanitizeFilename,
  suggestedFilename as filename,
  torrentSource,
  providerSource,
  isTorrentTask,
  capturedMedia,
  approvalSource,
} from "../downloads";
import { CollectionPicker } from "./CollectionPicker";
import { Choice } from "./primitives";
import { MediaExtras, type MediaExtrasValue } from "./MediaExtras";
function proposalDirectory(proposal?: Task) {
  return (
    proposal?.destination
      .replace(/[\\/][^\\/]+$/, "")
      .replace(/^\\\\\?\\/, "") || ""
  );
}
export function DownloadModal({
  visible,
  onClose,
  onAdded,
  onError,
  initialUrl = "",
  proposal,
}: {
  visible: boolean;
  onClose: (value: boolean) => void;
  onAdded: () => Promise<void>;
  onError: (value: string) => void;
  initialUrl?: string;
  proposal?: Task;
}) {
  const { t } = useTranslation();
  const [extras, setExtras] = useState<MediaExtrasValue>({
    selected: [],
    subtitleMode: "none",
    normalize: false,
    chapters: true,
  });
  const [mode, setMode] = useState("single");
  const [url, setUrl] = useState(initialUrl);
  const [name, setName] = useState("");
  const [container, setContainer] = useState("mp4");
  const [segments, setSegments] = useState("");
  const [streams, setStreams] = useState<MediaInfo | null>(null);
  const [video, setVideo] = useState("auto");
  const [audio, setAudio] = useState("auto");
  const [busy, setBusy] = useState(false);
  const [inspecting, setInspecting] = useState(false);
  const [localError, setLocalError] = useState("");
  const [directory, setDirectory] = useState(() => proposalDirectory(proposal));
  const [directoryEdited, setDirectoryEdited] = useState(false);
  const [later, setLater] = useState(false);
  const [resolved, setResolved] = useState<ResolvedMedia | null>(null);
  const [format, setFormat] = useState("0");
  const [connections, setConnections] = useState(() =>
    String(proposal?.request.connections ?? 0),
  );
  const connectionsEdited = useRef(false);
  const [transport, setTransport] = useState("auto");
  const [torrentPath, setTorrentPath] = useState("");
  const [collectionEntries, setCollectionEntries] = useState<CollectionEntry[]>(
    [],
  );
  const [collectionBusy, setCollectionBusy] = useState(false);
  const [audioTrack, setAudioTrack] = useState("auto");
  const inspectionGeneration = useRef(0);
  const providerUrl = proposal?.request.media?.provider?.url || url;
  const provider = providerSource(providerUrl);
  useEffect(() => {
    if (visible) {
      let active = true;
      inspectionGeneration.current++;
      connectionsEdited.current = false;
      if (proposal) {
        setConnections(String(proposal.request.connections));
        setDirectory(proposalDirectory(proposal));
      }
      setDirectoryEdited(false);
      const source = proposal?.request.url || initialUrl;
      const adaptive =
        !!proposal?.request.media ||
        providerSource(source) ||
        /\.(m3u8|mpd)(?:\?|$)/i.test(source);
      setMode(
        (proposal && isTorrentTask(proposal)) || torrentSource(source)
          ? "torrent"
          : adaptive
            ? "media"
            : "single",
      );
      setTorrentPath("");
      setCollectionEntries([]);
      setCollectionBusy(false);
      setInspecting(false);
      setAudioTrack(
        proposal?.request.media?.provider?.audio_format_id || "auto",
      );
      setContainer("mp4");
      setTransport(proposal?.request.transport || "auto");
      setSegments("");
      setVideo("auto");
      setAudio("auto");
      setUrl(source);
      setName(
        proposal?.request.filename ||
          (source ? filename(source, adaptive) : ""),
      );
      setVideo(
        proposal?.request.media?.video_stream == null
          ? "auto"
          : String(proposal.request.media.video_stream),
      );
      setAudio(
        proposal?.request.media?.audio_stream == null
          ? "auto"
          : String(proposal.request.media.audio_stream),
      );
      setContainer(proposal?.request.media?.container || "mp4");
      setStreams(null);
      setResolved(capturedMedia(proposal));
      setFormat("0");
      setExtras({
        selected: (proposal?.request.media?.subtitles || []).map(
          (_, index) => index,
        ),
        subtitleMode: proposal?.request.media?.subtitle_mode || "none",
        normalize: proposal?.request.media?.normalize_audio || false,
        chapters: true,
      });
      setLocalError("");
      void api
        .settings()
        .then((value) => {
          if (active && !proposal && !connectionsEdited.current)
            setConnections(String(value.connections));
        })
        .catch((error) => {
          if (active) onError(String(error));
        });
      return () => {
        active = false;
        inspectionGeneration.current++;
      };
    }
  }, [visible, initialUrl, onError, proposal]);
  useEffect(() => {
    if (!visible || directoryEdited || proposal || !desktop) return;
    let active = true;
    void api
      .suggestDirectory(name)
      .then((value) => {
        if (active) setDirectory(value);
      })
      .catch(() => {});
    return () => {
      active = false;
    };
  }, [visible, name, directoryEdited, proposal]);
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    setBusy(true);
    setLocalError("");
    const timestamp = (key: string) => {
      const value = String(form.get(key) || "");
      return value ? Math.floor(new Date(value).getTime() / 1000) : null;
    };
    try {
      if (["single", "media"].includes(mode) && provider && !resolved)
        throw new Error(t("inspect"));
      if (mode === "collection" && !collectionEntries.length)
        throw new Error(t("collection_empty"));
      if (mode === "torrent" && !torrentPath && !url.trim())
        throw new Error(t("torrent_source_hint"));
      const selected =
        mode === "media" ? resolved?.formats[Number(format)] : undefined;
      const torrentOptions: TorrentOptions = {
        upload_limit: Number(form.get("upload_limit") || 0),
        peer_limit: Number(form.get("peer_limit") || 80),
        only_files: proposal?.request.torrent?.only_files || [],
        initial_peers: String(form.get("initial_peers") || "")
          .split(/\r?\n/)
          .map((value) => value.trim())
          .filter(Boolean),
      };
      if (
        mode === "torrent" &&
        [Number(form.get("limit")), torrentOptions.upload_limit].some(
          (value) => value > 0 && value < 16384,
        )
      )
        throw new Error(t("torrent_min_limit", { count: 16384 }));
      const urls =
        mode === "batch"
          ? await api.expand(String(form.get("batch")))
          : [
              approvalSource(proposal, selected?.url, url) ||
                segments.trim().split(/\r?\n/)[0],
            ];
      if (!urls.length) throw new Error(t("no_results"));
      const names = new Set<string>();
      let inputs: Input[] = urls.map((source, index) => {
        let target = mode === "batch" ? filename(source, false) : name;
        if (names.has(target)) {
          const dot = target.lastIndexOf(".");
          target =
            dot > 0
              ? `${target.slice(0, dot)}-${index + 1}${target.slice(dot)}`
              : `${target}-${index + 1}`;
        }
        names.add(target);
        return {
          url: source,
          filename: target,
          connections: Number(connections),
          speed_limit: Number(form.get("limit")),
          expected_sha256: String(form.get("hash") || "").trim() || null,
          priority: Number(form.get("priority")),
          start_at: timestamp("start_at"),
          stop_at: timestamp("stop_at"),
          referer: proposal?.request.referer || null,
          auto_rename: true,
          browser_source: proposal?.request.browser_source || null,
          transport: transport as "auto" | "tcp" | "http3",
          content_type: proposal?.request.content_type || null,
          torrent: mode === "torrent" ? torrentOptions : null,
          media:
            mode === "media"
              ? {
                  ...proposal?.request.media,
                  provider:
                    selected && resolved
                      ? {
                          url: providerUrl,
                          format_id: selected.id || null,
                          height: selected.height,
                          audio_format_id:
                            audioTrack === "auto" ? null : audioTrack,
                        }
                      : proposal?.request.media?.provider || null,
                  segment_urls: segments
                    .split(/\r?\n/)
                    .map((value) => value.trim())
                    .filter(Boolean),
                  container: container as "mp4" | "mkv" | "mp3" | "mka",
                  subtitles: extras.selected
                    .map(
                      (index) =>
                        (streams?.subtitles ||
                          resolved?.subtitles ||
                          proposal?.request.media?.subtitles ||
                          [])[index],
                    )
                    .filter(Boolean),
                  subtitle_mode:
                    ["mp3", "mka"].includes(container) &&
                    extras.subtitleMode === "embed"
                      ? "external"
                      : extras.subtitleMode,
                  chapters: extras.chapters
                    ? streams?.chapters ||
                      resolved?.chapters ||
                      proposal?.request.media?.chapters ||
                      []
                    : [],
                  normalize_audio: container !== "mka" && extras.normalize,
                  kind:
                    proposal?.request.media?.kind ||
                    (streams?.format.includes("hls")
                      ? "hls"
                      : streams?.format.includes("dash")
                        ? "dash"
                        : "file"),
                  audio_url:
                    resolved?.audio_tracks?.find(
                      (track) => track.id === audioTrack,
                    )?.url ||
                    selected?.audio_url ||
                    String(form.get("audio_url") || "").trim() ||
                    proposal?.request.media?.audio_url ||
                    null,
                  duration_seconds:
                    Number(form.get("duration")) ||
                    proposal?.request.media?.duration_seconds ||
                    null,
                  video_stream: video === "auto" ? null : Number(video),
                  audio_stream: audio === "auto" ? null : Number(audio),
                }
              : null,
        };
      });
      if (mode === "collection")
        inputs = collectionInputs(collectionEntries, inputs[0]);
      let added: Task | undefined;
      if (mode === "torrent" && torrentPath)
        added = await api.importTorrent(
          torrentPath,
          name,
          directoryEdited ? directory : null,
          !later,
          torrentOptions,
          {
            speed_limit: inputs[0].speed_limit,
            priority: inputs[0].priority,
            start_at: inputs[0].start_at,
            stop_at: inputs[0].stop_at,
            expected_sha256: null,
          },
        );
      else if (proposal)
        added = await api.approve(proposal.id, inputs[0], directory, !later);
      else if (inputs.length === 1)
        added = await api.addTo(
          inputs[0],
          directoryEdited ? directory : null,
          !later,
        );
      else
        await api.addBatch(
          inputs,
          directoryEdited ? directory : undefined,
          !later,
        );
      await onAdded();
      if (added && !later && isTauri())
        await api
          .surface("details", added.id)
          .catch((error) => onError(String(error)));
      onClose(false);
    } catch (error) {
      setLocalError(String(error));
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function inspect() {
    const current = ++inspectionGeneration.current;
    setInspecting(true);
    setLocalError("");
    try {
      if (provider) {
        const metadata = await api.resolve(providerUrl.trim(), proposal?.id);
        if (current !== inspectionGeneration.current) return;
        if (!metadata.formats.length) throw new Error(t("noMedia"));
        setResolved(metadata);
        setMode("media");
        const captured = proposal?.request.media?.provider;
        const preferred = metadata.formats.findIndex(
          (item) =>
            item.id === captured?.format_id ||
            item.alternative_ids?.includes(captured?.format_id || ""),
        );
        const sameHeight = metadata.formats.findIndex(
          (item) => item.height === captured?.height,
        );
        const index =
          preferred >= 0 ? preferred : sameHeight >= 0 ? sameHeight : 0;
        setFormat(String(index));
        if (!proposal) {
          setContainer(metadata.formats[index].container);
          setName(
            sanitizeFilename(metadata.title) +
              "." +
              metadata.formats[index].container,
          );
        }
      } else {
        const metadata = await api.inspect(
          url,
          proposal?.request.referer,
          proposal?.id,
        );
        if (current !== inspectionGeneration.current) return;
        setStreams(metadata);
      }
      setExtras((value) => ({ ...value, selected: [] }));
    } catch (error) {
      if (current === inspectionGeneration.current)
        setLocalError(String(error));
    } finally {
      if (current === inspectionGeneration.current) setInspecting(false);
    }
  }
  return (
    <Dialog.Root open={visible} onOpenChange={onClose}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content className="modal download-modal">
          <Dialog.Title>{t("new_download")}</Dialog.Title>
          <Dialog.Description>{t("add_description")}</Dialog.Description>
          <Dialog.Close
            className="modal-close icon-button"
            aria-label={t("close")}
          >
            <X size={18} />
          </Dialog.Close>
          <form onSubmit={(event) => void submit(event)}>
            <Tabs.Root
              value={mode}
              onValueChange={(value) => {
                setMode(value);
                inspectionGeneration.current++;
                setInspecting(false);
                if (value === "media" && name)
                  setName(
                    name.replace(/\.(m3u8|mpd|mp4|mkv|mp3|mka)$/i, "") +
                      "." +
                      container,
                  );
              }}
            >
              <Tabs.List className="tabs" aria-label={t("downloads")}>
                <Tabs.Trigger value="single" disabled={busy}>
                  {t("file")}
                </Tabs.Trigger>
                {!proposal && (
                  <Tabs.Trigger value="batch" disabled={busy}>
                    {t("batch")}
                  </Tabs.Trigger>
                )}
                {!proposal && (
                  <Tabs.Trigger value="collection" disabled={busy}>
                    {t("collection")}
                  </Tabs.Trigger>
                )}
                <Tabs.Trigger value="media" disabled={busy}>
                  {t("downloadMedia")}
                </Tabs.Trigger>
                <Tabs.Trigger value="torrent" disabled={busy}>
                  {t("torrent")}
                </Tabs.Trigger>
              </Tabs.List>
              <Tabs.Content value="batch">
                <label>
                  {t("source_url")}
                  <textarea
                    name="batch"
                    required={mode === "batch"}
                    rows={5}
                    placeholder="https://example.com/file[001-010].zip"
                  />
                </label>
              </Tabs.Content>
              <Tabs.Content value="collection">
                <CollectionPicker
                  onChange={setCollectionEntries}
                  onBusy={setCollectionBusy}
                />
              </Tabs.Content>
              <Tabs.Content value="torrent">
                <label>
                  {t("torrent_source")}
                  <div className="folder-field">
                    <input
                      value={torrentPath || url}
                      readOnly={!!torrentPath || !!proposal}
                      required={mode === "torrent"}
                      onChange={(event) => {
                        setUrl(event.target.value);
                        setName(filename(event.target.value, false));
                      }}
                      placeholder="magnet:?xt=urn:btih:"
                    />
                    {!proposal && (
                      <button
                        type="button"
                        aria-label={t("choose_torrent")}
                        onClick={async () => {
                          try {
                            const path = await open({
                              multiple: false,
                              filters: [
                                { name: "BitTorrent", extensions: ["torrent"] },
                              ],
                            });
                            if (typeof path === "string") {
                              setTorrentPath(path);
                              setName(
                                sanitizeFilename(
                                  path
                                    .split(/[\\/]/)
                                    .pop()
                                    ?.replace(/\.torrent$/i, "") || "torrent",
                                ),
                              );
                            }
                          } catch (error) {
                            setLocalError(String(error));
                          }
                        }}
                      >
                        <FolderOpen size={17} />
                      </button>
                    )}
                  </div>
                </label>
                {torrentPath && (
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => {
                      setTorrentPath("");
                      setUrl("");
                    }}
                  >
                    {t("reset")}
                  </button>
                )}
                <p className="form-hint">{t("torrent_source_hint")}</p>
                <label>
                  {t("torrent_folder")}
                  <input
                    required
                    value={name}
                    maxLength={200}
                    onChange={(event) => setName(event.target.value)}
                  />
                </label>
              </Tabs.Content>
              {["single", "media"].map((tab) => (
                <Tabs.Content key={tab} value={tab}>
                  <label>
                    {t("source_url")}
                    <input
                      name="url"
                      type="url"
                      required={mode !== "batch" && !segments.trim()}
                      value={url}
                      readOnly={!!proposal}
                      onChange={(event) => {
                        setUrl(event.target.value);
                        inspectionGeneration.current++;
                        setInspecting(false);
                        setName(filename(event.target.value, mode === "media"));
                        setStreams(null);
                        setResolved(null);
                        setVideo("auto");
                        setAudio("auto");
                        setAudioTrack("auto");
                        if (torrentSource(event.target.value))
                          setMode("torrent");
                        if (
                          mode === "single" &&
                          (providerSource(event.target.value) ||
                            /\.(m3u8|mpd)(?:\?|$)/i.test(event.target.value))
                        ) {
                          setMode("media");
                          setName(filename(event.target.value, true));
                        }
                      }}
                      placeholder="https://"
                    />
                  </label>
                  <label>
                    {t("filename")}
                    <input
                      name="filename"
                      required
                      value={name}
                      onChange={(event) => setName(event.target.value)}
                      maxLength={200}
                    />
                  </label>
                  {provider && (
                    <button
                      type="button"
                      className="secondary"
                      disabled={inspecting}
                      onClick={() => void inspect()}
                    >
                      {t(inspecting ? "inspecting" : "inspect")}
                    </button>
                  )}
                  {resolved && (
                    <label>
                      {t("quality")}
                      <Choice
                        label={t("quality")}
                        value={format}
                        onChange={(value) => {
                          setFormat(value);
                        }}
                        options={resolved.formats.map((value, index) => ({
                          value: String(index),
                          label: `${value.audio_only ? t("audio_only") : value.height ? `${value.height}p` : t("automatic")}${value.fps ? ` · ${value.fps} fps` : ""} · ${value.container.toUpperCase()}${value.size ? ` · ${(value.size / 1048576).toFixed(1)} MB` : ""}`,
                        }))}
                      />
                    </label>
                  )}
                  {!!resolved?.audio_tracks?.length && (
                    <label>
                      {t("audio_track")}
                      <Choice
                        label={t("audio_track")}
                        value={audioTrack}
                        onChange={setAudioTrack}
                        options={[
                          { value: "auto", label: t("automatic") },
                          ...resolved.audio_tracks.map((track) => ({
                            value: track.id,
                            label: `${track.language || t("audio_only")} · ${track.codec}${track.bitrate ? ` · ${Math.round(track.bitrate)} kb/s` : ""}`,
                          })),
                        ]}
                      />
                    </label>
                  )}
                  {resolved && (
                    <label>
                      {t("container")}
                      <Choice
                        label={t("container")}
                        value={container}
                        onChange={(value) => {
                          setContainer(value);
                          setName(
                            name.replace(/\.(mp4|mkv|mp3|mka)$/i, "") +
                              "." +
                              value,
                          );
                        }}
                        options={[
                          { value: "mp4", label: "MP4" },
                          { value: "mkv", label: "MKV" },
                          { value: "mp3", label: "MP3" },
                          { value: "mka", label: t("lossless_audio") },
                        ]}
                      />
                    </label>
                  )}
                  {tab === "media" && !resolved && !provider && (
                    <>
                      <div className="media-inspect">
                        <button
                          type="button"
                          disabled={!desktop || !url || inspecting}
                          onClick={() => void inspect()}
                        >
                          {t(inspecting ? "inspecting" : "inspect")}
                        </button>
                        <Choice
                          label={t("container")}
                          value={container}
                          onChange={(value) => {
                            setContainer(value);
                            setName(
                              name.replace(/\.(mp4|mkv|mp3|mka)$/i, "") +
                                `.${value}`,
                            );
                          }}
                          options={[
                            { value: "mp4", label: "MP4" },
                            { value: "mkv", label: "MKV" },
                            { value: "mp3", label: "MP3" },
                            { value: "mka", label: t("lossless_audio") },
                          ]}
                        />
                      </div>
                      {streams && (
                        <div className="form-grid">
                          <label>
                            {t("quality")}
                            <Choice
                              label={t("quality")}
                              value={video}
                              onChange={setVideo}
                              options={[
                                { value: "auto", label: t("automatic") },
                                ...streams.streams
                                  .filter(
                                    (stream) => stream.codec_type === "video",
                                  )
                                  .map((stream) => ({
                                    value: String(stream.index),
                                    label: `${stream.height ? `${stream.height}p · ` : ""}${stream.codec_name}`,
                                  })),
                              ]}
                            />
                          </label>
                          <label>
                            {t("audio_url")}
                            <Choice
                              label={t("audio_url")}
                              value={audio}
                              onChange={setAudio}
                              options={[
                                { value: "auto", label: t("automatic") },
                                ...streams.streams
                                  .filter(
                                    (stream) => stream.codec_type === "audio",
                                  )
                                  .map((stream) => ({
                                    value: String(stream.index),
                                    label: `${stream.language ?? ""} ${stream.codec_name}`,
                                  })),
                              ]}
                            />
                          </label>
                        </div>
                      )}
                      <details className="download-advanced">
                        <summary>{t("media_processing")}</summary>
                        <label>
                          {t("audio_url")}
                          <input
                            name="audio_url"
                            type="url"
                            placeholder="https://"
                          />
                        </label>
                        <label>
                          {t("segment_urls")}
                          <textarea
                            value={segments}
                            onChange={(event) =>
                              setSegments(event.target.value)
                            }
                            rows={3}
                            placeholder="https://example.com/segment-001.ts"
                          />
                        </label>
                        <label>
                          {t("duration_seconds")}
                          <input
                            name="duration"
                            type="number"
                            min={1}
                            max={86400}
                          />
                        </label>
                      </details>
                    </>
                  )}
                </Tabs.Content>
              ))}
            </Tabs.Root>
            {mode === "media" && (
              <MediaExtras
                tracks={
                  streams?.subtitles ||
                  resolved?.subtitles ||
                  proposal?.request.media?.subtitles ||
                  []
                }
                chapterCount={
                  (
                    streams?.chapters ||
                    resolved?.chapters ||
                    proposal?.request.media?.chapters ||
                    []
                  ).length
                }
                value={extras}
                onChange={setExtras}
                audioOnly={["mp3", "mka"].includes(container)}
                lossless={container === "mka"}
              />
            )}
            <label>
              {t("download_directory")}
              <div className="folder-field">
                <input
                  value={directory}
                  onChange={(event) => {
                    setDirectoryEdited(true);
                    setDirectory(event.target.value);
                  }}
                  required={!!proposal}
                />
                <button
                  type="button"
                  aria-label={t("choose_folder")}
                  onClick={async () => {
                    try {
                      const path = await open({
                        directory: true,
                        multiple: false,
                        defaultPath: directory || undefined,
                      });
                      if (typeof path === "string") {
                        setDirectoryEdited(true);
                        setDirectory(path);
                      }
                    } catch (error) {
                      setLocalError(String(error));
                    }
                  }}
                >
                  <FolderOpen size={17} />
                </button>
              </div>
            </label>
            <div className="form-grid">
              {mode !== "torrent" && (
                <label>
                  {t("connections")}
                  <Choice
                    label={t("connections")}
                    value={connections}
                    onChange={(value) => {
                      connectionsEdited.current = true;
                      setConnections(value);
                    }}
                    options={[
                      { value: "0", label: t("automatic") },
                      ...Array.from({ length: 32 }, (_, index) => ({
                        value: String(index + 1),
                        label: String(index + 1),
                      })),
                    ]}
                  />
                </label>
              )}
              <label>
                {t("speed_limit")}
                <input
                  name="limit"
                  type="number"
                  min={0}
                  defaultValue={proposal?.request.speed_limit ?? 0}
                  required
                />
              </label>
            </div>
            {mode === "torrent" && (
              <div className="form-grid">
                <label>
                  {t("upload_limit")}
                  <input
                    name="upload_limit"
                    type="number"
                    min={0}
                    defaultValue={proposal?.request.torrent?.upload_limit || 0}
                    required
                  />
                </label>
                <label>
                  {t("peer_limit")}
                  <input
                    name="peer_limit"
                    type="number"
                    min={1}
                    max={500}
                    defaultValue={proposal?.request.torrent?.peer_limit || 80}
                    required
                  />
                </label>
              </div>
            )}
            <details className="download-advanced">
              <summary>{t("details")}</summary>
              {mode !== "torrent" && (
                <label>
                  {t("network_transport")}
                  <Choice
                    label={t("network_transport")}
                    value={transport}
                    onChange={setTransport}
                    options={[
                      { value: "auto", label: t("transport_auto") },
                      { value: "http3", label: t("transport_quic") },
                      { value: "tcp", label: t("transport_tcp") },
                    ]}
                  />
                </label>
              )}
              {mode === "torrent" && (
                <label>
                  {t("initial_peers")}
                  <textarea
                    name="initial_peers"
                    rows={2}
                    defaultValue={
                      proposal?.request.torrent?.initial_peers.join("\n") || ""
                    }
                    placeholder="127.0.0.1:6881"
                  />
                </label>
              )}
              <div className="form-grid">
                <label>
                  {t("start_at")}
                  <input name="start_at" type="datetime-local" />
                </label>
                <label>
                  {t("stop_at")}
                  <input name="stop_at" type="datetime-local" />
                </label>
              </div>
              <div className="form-grid">
                <label>
                  {t("priority")}
                  <input
                    name="priority"
                    type="number"
                    min={-10}
                    max={10}
                    defaultValue={0}
                    required
                  />
                </label>
                <label>
                  {t("expected_hash")}
                  <input
                    name="hash"
                    pattern="[a-fA-F0-9]{64}"
                    disabled={["batch", "collection", "torrent"].includes(mode)}
                  />
                </label>
              </div>
            </details>
            {localError && (
              <p className="form-error" role="alert">
                {localError}
              </p>
            )}
            <button
              className="primary"
              disabled={
                busy ||
                collectionBusy ||
                !desktop ||
                (["single", "media"].includes(mode) && provider && !resolved) ||
                (mode === "collection" && !collectionEntries.length)
              }
              type="submit"
            >
              <Download size={16} />
              {t(busy ? "starting" : "start")}
            </button>
            <label className="queue-later">
              <input
                type="checkbox"
                checked={later}
                onChange={(event) => setLater(event.target.checked)}
              />
              {t("download_later")}
            </label>
          </form>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
