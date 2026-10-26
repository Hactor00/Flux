import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as AlertDialog from "@radix-ui/react-alert-dialog";
import {
  Copy,
  FolderOpen,
  Trash2,
  X,
  Pause,
  Play,
  SlidersHorizontal,
  Minus,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { api, desktop, type Task, type Progress, taskFilename } from "../api";
import { transferView, formatDuration } from "../transfer";
import { isTorrentTask } from "../downloads";
import { TorrentDetails } from "./TorrentDetails";
import { useSurfaceSize } from "../use-surface-size";
export function TaskDetails({
  task,
  progress,
  onClose,
  onChanged,
  onError,
}: {
  task: Task | null;
  progress?: Progress;
  onClose: () => void;
  onChanged: () => Promise<void>;
  onError: (value: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const [expanded, setExpanded] = useState(false);
  const [closeFinished, setCloseFinished] = useState(
    () => localStorage.getItem("flux-close-finished-progress") === "true",
  );
  const windowContent = useRef<HTMLDivElement>(null);
  const wasActive = useRef(false);
  const activeTaskId = useRef<string | null>(null);
  useSurfaceSize(windowContent, !!task);
  const [priority, setPriority] = useState(0);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [preview, setPreview] = useState("");
  const [previewBusy, setPreviewBusy] = useState(false);
  const [previewNotice, setPreviewNotice] = useState("");
  const previewRequest = useRef(0);
  const view = task ? transferView(task, progress) : null;
  const percent = view?.percent;
  useEffect(() => {
    if (!task) {
      wasActive.current = false;
      activeTaskId.current = null;
      return;
    }
    if (activeTaskId.current !== task.id) {
      activeTaskId.current = task.id;
      wasActive.current = false;
    }
    if (task.status !== "complete") wasActive.current = true;
    if (task.status === "complete" && wasActive.current && closeFinished)
      onClose();
  }, [task, closeFinished, onClose]);
  const size = (value: number) => {
    const index = Math.min(4, Math.floor(Math.log2(Math.max(1, value)) / 10));
    return (
      new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(
        value / 1024 ** index,
      ) +
      " " +
      ["B", "KB", "MB", "GB", "TB"][index]
    );
  };
  useEffect(() => {
    setPriority(task?.request.priority ?? 0);
    setNotice("");
    setPreview("");
    setPreviewNotice("");
    setPreviewBusy(false);
    previewRequest.current++;
    return () => {
      previewRequest.current++;
    };
  }, [task?.id, task?.request.priority]);
  async function action(operation: () => Promise<unknown>) {
    setBusy(true);
    try {
      await operation();
      await onChanged();
    } catch (error) {
      setNotice(String(error));
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }
  const date = (value: number | null | undefined) =>
    value
      ? new Intl.DateTimeFormat(i18n.language, {
          dateStyle: "medium",
          timeStyle: "short",
        }).format(value * 1000)
      : "—";
  return (
    <Dialog.Root
      open={!!task}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content
          className={`modal details-modal compact-transfer ${expanded ? "is-expanded" : ""}`}
        >
          <div className="transfer-window-content" ref={windowContent}>
            <Dialog.Title>{task ? taskFilename(task) : ""}</Dialog.Title>
            <Dialog.Description>{t("details")}</Dialog.Description>
            <Dialog.Close
              className="modal-close icon-button"
              aria-label={t("close")}
            >
              <X size={18} />
            </Dialog.Close>
            {task && (
              <>
                <div className="transfer-summary">
                  <div>
                    <strong
                      title={
                        view?.percentEstimated ? t("estimated_size") : undefined
                      }
                    >
                      {percent == null
                        ? "—"
                        : `${view?.percentEstimated ? "≈ " : ""}${percent.toFixed(1)}%`}
                    </strong>
                    <span>
                      {size(view?.done || 0)} /{" "}
                      {view?.total != null
                        ? `${view.estimated ? "≈ " : ""}${size(view.total)}`
                        : t("unknown_size")}
                    </span>
                  </div>
                  <div
                    className={`progress-track ${percent == null ? "indeterminate" : ""}`}
                    role="progressbar"
                    aria-label={t("progress")}
                    aria-valuemin={0}
                    aria-valuemax={100}
                    aria-valuenow={percent ?? undefined}
                  >
                    <div
                      style={{ width: percent == null ? "35%" : `${percent}%` }}
                    />
                  </div>
                  <small>
                    {t(
                      view?.phase === "processing"
                        ? "media_finalizing"
                        : view?.phase === "live"
                          ? "live_stream"
                          : task.status,
                    )}{" "}
                    · {size(view?.rate || 0)}/s
                  </small>
                  <div className="transfer-timing" aria-live="off">
                    <span>
                      {t("remaining")}:{" "}
                      <strong>
                        {view?.phase === "processing"
                          ? t("media_finalizing")
                          : view?.phase === "verifying"
                            ? t("verifying")
                            : view?.phase === "live"
                              ? t("live_stream")
                              : view?.eta != null
                                ? `≈ ${formatDuration(view.eta)}`
                                : task.status === "running"
                                  ? t("calculating_time")
                                  : "—"}
                      </strong>
                    </span>
                    {view?.media && (
                      <span>
                        {t("media_time")}:{" "}
                        {formatDuration(view.media.processed_ms / 1000)}
                        {view.media.duration_ms != null
                          ? ` / ${formatDuration(view.media.duration_ms / 1000)}`
                          : ""}
                      </span>
                    )}
                    {view?.media?.segments_total != null && (
                      <span>
                        {t("segments")}: {view.media.segments_completed} /{" "}
                        {view.media.segments_total}
                      </span>
                    )}
                  </div>
                </div>
                <div className="transfer-expanded" hidden={!expanded}>
                  {(task.request.media ||
                    /\.(mp4|mkv|webm|mov|ts)$/i.test(task.destination)) && (
                    <div className="media-preview">
                      <div className="media-preview-heading">
                        <strong>{t("media_preview")}</strong>
                        <button
                          type="button"
                          className="secondary"
                          disabled={
                            previewBusy || !desktop || task.completed === 0
                          }
                          onClick={() => {
                            const id = task.id,
                              generation = ++previewRequest.current;
                            setPreviewBusy(true);
                            setPreviewNotice("");
                            void api
                              .previewFrame(id)
                              .then((image) => {
                                if (generation === previewRequest.current)
                                  setPreview(image);
                              })
                              .catch(() => {
                                if (generation === previewRequest.current)
                                  setPreviewNotice(t("preview_not_ready"));
                              })
                              .finally(() => {
                                if (generation === previewRequest.current)
                                  setPreviewBusy(false);
                              });
                          }}
                        >
                          {t(
                            previewBusy
                              ? "inspecting"
                              : preview
                                ? "refresh_preview"
                                : "media_preview",
                          )}
                        </button>
                      </div>
                      {preview && (
                        <img src={preview} alt={t("media_preview")} />
                      )}
                      <p className="form-hint">
                        {previewNotice || t("preview_download_hint")}
                      </p>
                    </div>
                  )}
                  <dl className="task-details">
                    <dt>{t("status")}</dt>
                    <dd>{t(task.status)}</dd>
                    <dt>{t("source_url")}</dt>
                    <dd className="source-address" title={task.request.url}>
                      {task.request.media?.provider?.url || task.request.url}
                    </dd>
                    <dt>{t("download_directory")}</dt>
                    <dd>{task.destination}</dd>
                    <dt>{t("source_type")}</dt>
                    <dd>
                      {t(
                        isTorrentTask(task)
                          ? "torrent"
                          : task.request.media
                            ? "downloadMedia"
                            : "file",
                      )}
                    </dd>
                    {!isTorrentTask(task) && (
                      <>
                        <dt>{t("connections")}</dt>
                        <dd>{task.request.connections || t("automatic")}</dd>
                      </>
                    )}
                    <dt>{t("created_at")}</dt>
                    <dd>{date(task.created_at)}</dd>
                    <dt>{t("started_at")}</dt>
                    <dd>{date(task.started_at)}</dd>
                    <dt>{t("finished_at")}</dt>
                    <dd
                      title={
                        task.timestamp_estimated
                          ? t("estimated_time")
                          : undefined
                      }
                    >
                      {task.timestamp_estimated ? "≈ " : ""}
                      {date(task.finished_at)}
                    </dd>
                    {task.request.start_at && (
                      <>
                        <dt>{t("start_at")}</dt>
                        <dd>{date(task.request.start_at)}</dd>
                      </>
                    )}
                    {task.request.stop_at && (
                      <>
                        <dt>{t("stop_at")}</dt>
                        <dd>{date(task.request.stop_at)}</dd>
                      </>
                    )}
                    <dt>SHA-256</dt>
                    <dd>
                      {task.sha256 ?? "—"}
                      {task.sha256 && (
                        <button
                          className="icon-button"
                          aria-label={t("copy_hash")}
                          onClick={() =>
                            void action(async () => {
                              await writeText(task.sha256!);
                              setNotice(t("saved"));
                            })
                          }
                        >
                          <Copy size={14} />
                        </button>
                      )}
                    </dd>
                  </dl>
                  {isTorrentTask(task) && (
                    <TorrentDetails
                      task={task}
                      busy={busy || !desktop}
                      size={size}
                      onApply={(options, speedLimit) =>
                        action(() =>
                          api.torrentOptions(task.id, options, speedLimit),
                        )
                      }
                    />
                  )}
                  <label>
                    {t("priority")}
                    <div className="priority-field">
                      <input
                        type="number"
                        min={-10}
                        max={10}
                        value={priority}
                        disabled={["running", "verifying"].includes(
                          task.status,
                        )}
                        onChange={(event) =>
                          setPriority(Number(event.target.value))
                        }
                      />
                      <button
                        className="secondary"
                        disabled={
                          busy ||
                          !desktop ||
                          ["running", "verifying"].includes(task.status)
                        }
                        onClick={() =>
                          void action(() => api.priority(task.id, priority))
                        }
                      >
                        {t("apply")}
                      </button>
                    </div>
                  </label>
                </div>
                {task.error && (
                  <p className="form-error" role="alert">
                    {task.error}
                  </p>
                )}
                <div className="details-actions transfer-actions">
                  <button
                    className="secondary transfer-details-button"
                    aria-expanded={expanded}
                    onClick={() => setExpanded((value) => !value)}
                  >
                    <SlidersHorizontal size={15} />
                    {t("details")}
                  </button>
                  <label className="transfer-auto-close">
                    <input
                      type="checkbox"
                      checked={closeFinished}
                      onChange={(event) => {
                        setCloseFinished(event.target.checked);
                        localStorage.setItem(
                          "flux-close-finished-progress",
                          String(event.target.checked),
                        );
                      }}
                    />
                    {t("auto_close_progress")}
                  </label>
                  <span className="toolbar-spacer" />
                  {(!["complete", "cancelled", "awaiting_approval"].includes(
                    task.status,
                  ) ||
                    !!task.torrent) && (
                    <button
                      className="secondary"
                      disabled={busy || !desktop}
                      onClick={() =>
                        void action(() =>
                          task.status === "running" ||
                          task.status === "verifying" ||
                          task.torrent?.state === "seeding"
                            ? api.pause(task.id)
                            : api.resume(task.id),
                        )
                      }
                    >
                      {["running", "verifying"].includes(task.status) ||
                      task.torrent?.state === "seeding" ? (
                        <Pause size={15} />
                      ) : (
                        <Play size={15} />
                      )}
                      {t(
                        ["running", "verifying"].includes(task.status) ||
                          task.torrent?.state === "seeding"
                          ? "pause"
                          : "resume",
                      )}
                    </button>
                  )}
                  {task.status === "complete" && (
                    <>
                      <button
                        className="secondary"
                        onClick={() => void action(() => api.open(task.id))}
                      >
                        {t(isTorrentTask(task) ? "open_folder" : "open_file")}
                      </button>
                      <button
                        className="secondary"
                        onClick={() =>
                          void action(() => api.open(task.id, true))
                        }
                      >
                        <FolderOpen size={15} />
                        {t("reveal_file")}
                      </button>
                    </>
                  )}
                  <AlertDialog.Root>
                    <AlertDialog.Trigger asChild>
                      <button
                        className="danger secondary"
                        disabled={busy || !desktop}
                      >
                        <Trash2 size={15} />
                        {t("remove")}
                      </button>
                    </AlertDialog.Trigger>
                    <AlertDialog.Portal>
                      <AlertDialog.Overlay className="modal-overlay" />
                      <AlertDialog.Content className="modal confirm-modal">
                        <AlertDialog.Title>{t("remove")}</AlertDialog.Title>
                        <AlertDialog.Description>
                          {t("remove_description")}
                        </AlertDialog.Description>
                        <div className="dialog-actions">
                          <AlertDialog.Cancel asChild>
                            <button className="secondary">{t("close")}</button>
                          </AlertDialog.Cancel>
                          <AlertDialog.Action asChild>
                            <button
                              className="danger secondary"
                              onClick={() =>
                                void action(async () => {
                                  await api.remove(task.id);
                                  onClose();
                                })
                              }
                            >
                              {t("confirm")}
                            </button>
                          </AlertDialog.Action>
                        </div>
                      </AlertDialog.Content>
                    </AlertDialog.Portal>
                  </AlertDialog.Root>
                  <button
                    className="secondary transfer-background"
                    onClick={onClose}
                  >
                    <Minus size={15} />
                    {t("background_download")}
                  </button>
                </div>
                <p role="status">{notice}</p>
              </>
            )}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
