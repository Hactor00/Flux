import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useTranslation } from "react-i18next";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { getVersion } from "@tauri-apps/api/app";
import { relaunch } from "@tauri-apps/plugin-process";
import { api, desktop } from "../api";
import { compareVersions } from "../update-version.js";
import { releaseUpdateResource } from "../update-resource";
import { ArrowDownToLine, RefreshCw } from "lucide-react";

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
  const [linux, setLinux] = useState(false);
  const [portable, setPortable] = useState(false);
  const [releaseVersion, setReleaseVersion] = useState("");
  const [installed, setInstalled] = useState("");
  async function inspect(silent = false) {
    if (!desktop || request.current) return;
    request.current = true;
    setBusy(true);
    if (!silent) setNotice(t("checking_updates"));
    try {
      const installedVersion = await getVersion();
      if (alive.current) setInstalled(installedVersion);
      const isLinux = (await api.runtimePlatform()) === "linux";
      const isPortable = (await api.installationMode()) === "portable";
      if (isLinux || isPortable) {
        const latest = await api.latestReleaseVersion();
        const newer = compareVersions(latest, installedVersion) === 1;
        if (alive.current) {
          setLinux(isLinux);
          setPortable(isPortable);
          setReleaseVersion(newer ? latest : "");
          setVisible(newer);
          if (!silent) setNotice(newer ? "" : t("up_to_date"));
        }
        return;
      }
      const next = await check({ timeout: 15000 });
      if (!alive.current) {
        await releaseUpdateResource(next);
        return;
      }
      const previous = resource.current;
      resource.current = null;
      await releaseUpdateResource(previous);
      if (!alive.current) {
        await releaseUpdateResource(next);
        return;
      }
      const versionOrder = next
        ? compareVersions(next.version, installedVersion)
        : null;
      if (next && (versionOrder === null || versionOrder <= 0)) {
        await releaseUpdateResource(next);
        resource.current = null;
        setUpdate(null);
        setVisible(false);
        if (!silent) setNotice(t("up_to_date"));
        return;
      }
      resource.current = next;
      setUpdate(next);
      if (next) {
        setNotice("");
        setVisible(true);
      } else {
        setVisible(false);
        if (!silent) setNotice(t("up_to_date"));
      }
    } catch (error) {
      if (!silent && alive.current) setNotice(String(error));
    } finally {
      request.current = false;
      if (alive.current) setBusy(false);
    }
  }
  useEffect(() => {
    alive.current = true;
    if (desktop)
      void getVersion()
        .then((version) => {
          if (alive.current) setInstalled(version);
        })
        .catch(() => undefined);
    if (desktop)
      void api
        .runtimePlatform()
        .then((platform) => {
          if (alive.current) setLinux(platform === "linux");
        })
        .catch(() => undefined);
    const timer = startup
      ? setTimeout(() => {
          void inspect(true);
        }, 3000)
      : undefined;
    return () => {
      alive.current = false;
      clearTimeout(timer);
      const previous = resource.current;
      resource.current = null;
      void releaseUpdateResource(previous);
    };
  }, [startup]);
  async function install() {
    if ((linux || portable) && releaseVersion) {
      await api.openReleasePage().catch((error) => setNotice(String(error)));
      return;
    }
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
      const platform = await api.runtimePlatform();
      await update.install({ restartAfterInstall: platform === "windows" });
      if (platform !== "windows") await relaunch();
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
          {installed && (
            <p>
              <span className="muted">{t("installed_version")}</span> ·{" "}
              <strong>{installed}</strong>
            </p>
          )}
          <p className="muted">
            {t(
              portable
                ? "portable_updates_help"
                : linux
                  ? "rpm_updates_help"
                  : "updates_help",
            )}
          </p>
          <button
            className="secondary"
            disabled={!desktop || busy}
            onClick={() => void inspect()}
          >
            <RefreshCw size={16} className={busy ? "spinning" : undefined} />
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
          <Dialog.Overlay className="modal-overlay update-overlay" />
          <Dialog.Content
            className="modal updater-modal"
            onEscapeKeyDown={(event) => {
              if (busy) event.preventDefault();
            }}
            onPointerDownOutside={(event) => {
              if (busy) event.preventDefault();
            }}
          >
            <div className="updater-body">
              <span className="update-symbol" aria-hidden="true">
                <ArrowDownToLine size={25} />
              </span>
              <Dialog.Title>
                {t("update_available")} ·{" "}
                {linux || portable ? releaseVersion : update?.version}
              </Dialog.Title>
              <Dialog.Description>
                {installed && (
                  <>
                    {t("installed_version")} · {installed}
                    <br />
                  </>
                )}
                {t(
                  portable
                    ? "portable_updates_help"
                    : linux
                      ? "rpm_updates_help"
                      : "update_description",
                )}
              </Dialog.Description>
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
              <p role="status" className="update-status">
                {notice}
                {busy && progress !== null ? ` ${progress.toFixed(0)}%` : ""}
              </p>
            </div>
            <div className="dialog-actions">
              <Dialog.Close className="secondary" disabled={busy}>
                {t("later")}
              </Dialog.Close>
              <button
                className="primary"
                disabled={busy}
                onClick={() => void install()}
              >
                <ArrowDownToLine size={16} />
                {t(linux || portable ? "release_downloads" : "update_now")}
              </button>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </>
  );
}
