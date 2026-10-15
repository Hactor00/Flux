import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useTranslation } from "react-i18next";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { api, desktop } from "../api";

export function Updater({ startup = false }: { startup?: boolean }) {
  const { t } = useTranslation();
  const [update, setUpdate] = useState<Update | null>(null);
  const resource = useRef<Update | null>(null);
  const request = useRef(false);
  const alive = useRef(true);
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [downloaded, setDownloaded] = useState(0);
  const [total, setTotal] = useState<number | null>(null);
  async function inspect(silent = false) {
    if (!desktop || request.current) return;
    request.current = true;
    setBusy(true);
    if (!silent) setNotice(t("checking_updates"));
    try {
      const next = await check({ timeout: 15000 });
      if (!alive.current) {
        await next?.close();
        return;
      }
      await resource.current?.close();
      resource.current = next;
      setUpdate(next);
      if (next) {
        setNotice("");
        setVisible(true);
      } else if (!silent) setNotice(t("up_to_date"));
    } catch (error) {
      if (!silent && alive.current) setNotice(String(error));
    } finally {
      request.current = false;
      if (alive.current) setBusy(false);
    }
  }
  useEffect(() => {
    alive.current = true;
    const timer = startup
      ? setTimeout(() => {
          void inspect(true);
        }, 3000)
      : undefined;
    return () => {
      alive.current = false;
      clearTimeout(timer);
      void resource.current?.close();
      resource.current = null;
    };
  }, [startup]);
  async function install() {
    if (!update || busy) return;
    setBusy(true);
    setNotice(t("downloading_update"));
    setDownloaded(0);
    let prepared = false;
    try {
      await update.download(
        (event) => {
          if (event.event === "Started")
            setTotal(event.data.contentLength ?? null);
          if (event.event === "Progress")
            setDownloaded((previous) => previous + event.data.chunkLength);
        },
        { timeout: 120000 },
      );
      if (!alive.current) return;
      setNotice(t("installing_update"));
      await api.prepareUpdate();
      prepared = true;
      await update.install({ restartAfterInstall: true });
      await relaunch();
    } catch (error) {
      if (prepared) await api.abortUpdate().catch(() => undefined);
      setNotice(String(error));
      setBusy(false);
    }
  }
  const progress = total ? Math.min(100, (downloaded / total) * 100) : null;
  return (
    <>
      {!startup && (
        <div className="updater-panel">
          <p className="muted">{t("updates_help")}</p>
          <button disabled={!desktop || busy} onClick={() => void inspect()}>
            {t("check_updates")}
          </button>
          {notice && !visible && <p role="status">{notice}</p>}
        </div>
      )}
      <Dialog.Root
        open={visible}
        onOpenChange={(value) => {
          if (!busy) setVisible(value);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="modal-overlay" />
          <Dialog.Content
            className="modal updater-modal"
            onEscapeKeyDown={(event) => {
              if (busy) event.preventDefault();
            }}
            onPointerDownOutside={(event) => {
              if (busy) event.preventDefault();
            }}
          >
            <Dialog.Title>
              {t("update_available")} · {update?.version}
            </Dialog.Title>
            <Dialog.Description>{t("update_description")}</Dialog.Description>
            <pre className="update-notes">
              {update?.body?.slice(0, 16384) || t("update_description")}
            </pre>
            {busy && (
              <progress
                aria-label={t("downloading_update")}
                max={100}
                value={progress ?? undefined}
              />
            )}
            <p role="status">
              {notice}
              {busy && progress !== null ? ` ${progress.toFixed(0)}%` : ""}
            </p>
            <div className="dialog-actions">
              <Dialog.Close disabled={busy}>{t("later")}</Dialog.Close>
              <button
                className="primary"
                disabled={busy}
                onClick={() => void install()}
              >
                {t("update_now")}
              </button>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </>
  );
}
