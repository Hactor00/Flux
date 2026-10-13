import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { Task, TorrentOptions } from "../api";
import { SelectionList } from "./SelectionList";
import { SeedingRules } from "./SeedingRules";
import type { SeedingRule } from "../api";

export function TorrentDetails({
  task,
  busy,
  onApply,
  size,
}: {
  task: Task;
  busy: boolean;
  onApply: (options: TorrentOptions, speedLimit: number) => Promise<void>;
  size: (value: number) => string;
}) {
  const { t, i18n } = useTranslation();
  const [downloadLimit, setDownloadLimit] = useState(0);
  const [seeding, setSeeding] = useState<SeedingRule | null>(
    task.request.torrent?.seeding ?? null,
  );
  const [uploadLimit, setUploadLimit] = useState(0);
  const [peerLimit, setPeerLimit] = useState(80);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [filesEdited, setFilesEdited] = useState(false);
  const snapshot = task.torrent;
  const options = task.request.torrent;
  const selectionKey = options?.only_files.join(",") || "";
  useEffect(() => {
    setDownloadLimit(task.request.speed_limit);
    setUploadLimit(options?.upload_limit || 0);
    setPeerLimit(options?.peer_limit || 80);
    setFilesEdited(false);
  }, [
    task.id,
    task.request.speed_limit,
    options?.upload_limit,
    options?.peer_limit,
    selectionKey,
  ]);
  useEffect(() => {
    if (!filesEdited)
      setSelected(
        new Set(
          snapshot?.files
            .filter((file) => file.selected)
            .map((file) => file.index) || [],
        ),
      );
  }, [snapshot?.files, filesEdited]);
  const states: Record<string, string> = {
    resolving: "torrent_resolving",
    initializing: "torrent_initializing",
    downloading: "running",
    paused: "paused",
    verifying: "verifying",
    complete: "complete",
    seeding: "seeding",
    seed_paused: "paused",
    seed_failed: "failed",
    failed: "failed",
  };
  return (
    <section className="torrent-details">
      <h3>{t("torrent")}</h3>
      <dl className="task-details">
        <dt>{t("status")}</dt>
        <dd>{t(states[snapshot?.state || "resolving"] || "status")}</dd>
        <dt>{t("info_hash")}</dt>
        <dd className="torrent-info-hash">{snapshot?.info_hash || "—"}</dd>
        <dt>{t("peers")}</dt>
        <dd>
          {new Intl.NumberFormat(i18n.language).format(snapshot?.peers || 0)}
        </dd>
        <dt>{t("uploaded")}</dt>
        <dd>{size(snapshot?.uploaded || 0)}</dd>
        <dt>{t("upload_speed")}</dt>
        <dd>{size(snapshot?.upload_bps || 0)}/s</dd>
        <dt>{t("ratio")}</dt>
        <dd>
          {new Intl.NumberFormat(i18n.language, {
            maximumFractionDigits: 3,
          }).format((snapshot?.uploaded || 0) / Math.max(1, task.completed))}
        </dd>
        <dt>{t("seed_minutes")}</dt>
        <dd>{Math.floor((snapshot?.seeded_seconds || 0) / 60)}</dd>
      </dl>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          const files = snapshot?.files || [];
          const onlyFiles = filesEdited
            ? selected.size === files.length
              ? []
              : [...selected].sort((a, b) => a - b)
            : options?.only_files || [];
          void onApply(
            {
              seeding,
              upload_limit: uploadLimit,
              peer_limit: peerLimit,
              only_files: onlyFiles,
              initial_peers: options?.initial_peers || [],
            },
            downloadLimit,
          );
        }}
      >
        <SeedingRules value={seeding} onChange={setSeeding} inherit />
        <div className="form-grid">
          <label>
            {t("speed_limit")}
            <input
              type="number"
              min={0}
              required
              value={downloadLimit}
              onChange={(event) => {
                const value = Number(event.target.value);
                event.target.setCustomValidity(
                  value > 0 && value < 16384
                    ? t("torrent_min_limit", { count: 16384 })
                    : "",
                );
                setDownloadLimit(value);
              }}
            />
          </label>
          <label>
            {t("upload_limit")}
            <input
              type="number"
              min={0}
              required
              value={uploadLimit}
              onChange={(event) => {
                const value = Number(event.target.value);
                event.target.setCustomValidity(
                  value > 0 && value < 16384
                    ? t("torrent_min_limit", { count: 16384 })
                    : "",
                );
                setUploadLimit(value);
              }}
            />
          </label>
        </div>
        <label>
          {t("peer_limit")}
          <input
            type="number"
            min={1}
            max={500}
            required
            value={peerLimit}
            onChange={(event) => setPeerLimit(Number(event.target.value))}
          />
        </label>
        {!!snapshot?.files.length && (
          <>
            <div className="collection-heading">
              <strong>{t("torrent_files")}</strong>
              <span>{t("collection_selected", { count: selected.size })}</span>
            </div>
            <SelectionList
              label={t("torrent_files")}
              disabled={task.status === "complete" || busy}
              items={snapshot.files.map((file) => ({
                key: file.index,
                title: file.path,
                detail: size(file.length),
                selected: selected.has(file.index),
                hash: file.sha256,
              }))}
              onToggle={(index, checked) => {
                setFilesEdited(true);
                setSelected((value) => {
                  const next = new Set(value);
                  if (checked) next.add(index);
                  else next.delete(index);
                  return next;
                });
              }}
            />
          </>
        )}
        <p className="form-hint">{t("torrent_limits_hint")}</p>
        <button
          className="secondary"
          disabled={busy || (!!snapshot?.files.length && selected.size === 0)}
          type="submit"
        >
          {t("apply")}
        </button>
      </form>
    </section>
  );
}
