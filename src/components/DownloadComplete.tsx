import { useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { Check, FolderOpen, Play, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api, taskFilename, type Task } from "../api";
import { isTorrentTask } from "../downloads";

export function DownloadComplete({
  task,
  onClose,
}: {
  task: Task;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  async function open(reveal = false) {
    setBusy(true);
    setError("");
    try {
      await api.open(task.id, reveal);
      onClose();
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  return (
    <Dialog.Root
      open
      onOpenChange={(value) => {
        if (!value) onClose();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content className="modal download-complete">
          <div className="completion-icon">
            <Check size={26} />
          </div>
          <Dialog.Title>{t("download_finished")}</Dialog.Title>
          <Dialog.Description>{taskFilename(task)}</Dialog.Description>
          <Dialog.Close
            className="modal-close icon-button"
            aria-label={t("close")}
          >
            <X size={18} />
          </Dialog.Close>
          <p className="completion-path" title={task.destination}>
            {task.destination.replace(/^\\\\\?\\/, "")}
          </p>
          {error && (
            <p className="form-error" role="alert">
              {error}
            </p>
          )}
          <div className="details-actions">
            <button
              className="primary"
              disabled={busy}
              onClick={() => void open()}
            >
              <Play size={16} />
              {t(isTorrentTask(task) ? "open_folder" : "open_file")}
            </button>
            <button
              className="secondary"
              disabled={busy}
              onClick={() => void open(true)}
            >
              <FolderOpen size={16} />
              {t("reveal_file")}
            </button>
            <Dialog.Close className="secondary">{t("close")}</Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
