import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api, desktop, type Settings } from "../api";
import { Choice, Toggle } from "./primitives";

export function TorrentWatchSettings({
  value,
  onChange,
  onError,
}: {
  value: Settings["torrent_watch"];
  onChange: (value: Settings["torrent_watch"]) => void;
  onError: (error: string) => void;
}) {
  const { t } = useTranslation();
  const [report, setReport] = useState<{
    imported: number;
    error: string | null;
  } | null>(null);
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    const refresh = () =>
      void api
        .torrentWatchReport()
        .then((result) => {
          if (alive) setReport(result);
        })
        .catch((error) => {
          if (alive) onError(String(error));
        });
    refresh();
    const timer = setInterval(refresh, 5000);
    return () => {
      alive = false;
      clearInterval(timer);
    };
  }, [onError]);
  async function choose(destination = false) {
    try {
      const path = await open({ directory: true, multiple: false });
      if (typeof path !== "string") return;
      if (destination) onChange({ ...value, destination: path });
      else if (!value.folders.includes(path))
        onChange({ ...value, folders: [...value.folders, path] });
    } catch (error) {
      onError(String(error));
    }
  }
  return (
    <section className="torrent-watch-settings">
      <p className="field-hint">{t("watch_help")}</p>
      <Toggle
        label={t("watch_enabled")}
        checked={value.enabled}
        onChange={(enabled) => onChange({ ...value, enabled })}
      />
      {value.folders.map((folder) => (
        <div className="folder-field" key={folder}>
          <input aria-label={t("watch_title")} readOnly value={folder} />
          <button
            aria-label={t("remove")}
            onClick={() =>
              onChange({
                ...value,
                folders: value.folders.filter((path) => path !== folder),
              })
            }
          >
            <X size={17} />
          </button>
        </div>
      ))}
      <button
        className="secondary"
        disabled={!desktop || value.folders.length >= 16}
        onClick={() => void choose()}
      >
        <FolderOpen size={17} /> {t("watch_add")}
      </button>
      <label>
        {t("watch_mode")}
        <Choice
          label={t("watch_mode")}
          value={value.mode}
          onChange={(mode) =>
            onChange({ ...value, mode: mode as "approval" | "automatic" })
          }
          options={[
            { value: "approval", label: t("watch_approval") },
            { value: "automatic", label: t("watch_automatic") },
          ]}
        />
      </label>
      <label>
        {t("download_directory")}
        <div className="folder-field">
          <input readOnly value={value.destination || t("automatic")} />
          <button
            disabled={!desktop}
            aria-label={t("choose_folder")}
            onClick={() => void choose(true)}
          >
            <FolderOpen size={17} />
          </button>
          <button
            className="secondary"
            onClick={() => onChange({ ...value, destination: null })}
          >
            {t("reset")}
          </button>
        </div>
      </label>
      {report && (
        <p role="status">{t("watch_imported", { count: report.imported })}</p>
      )}
      {report?.error && <p role="alert">{report.error}</p>}
    </section>
  );
}
