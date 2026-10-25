import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, subscribe, type Task, type Progress, type Settings } from "./api";
import { DownloadModal } from "./components/DownloadModal";
import { SettingsModal } from "./components/SettingsModal";
import { TaskDetails } from "./components/TaskDetails";
import { PowerCountdown } from "./components/PowerCountdown";
import { DownloadComplete } from "./components/DownloadComplete";

export default function Surface() {
  const { i18n, t } = useTranslation();
  const query = new URLSearchParams(location.search);
  const view = query.get("view");
  const id = query.get("id");
  const [task, setTask] = useState<Task | undefined>();
  const [progress, setProgress] = useState<Progress>();
  const [error, setError] = useState("");
  const close = useCallback(() => {
    void api.closeSurface().catch((error) => setError(String(error)));
  }, []);
  useEffect(() => {
    document.body.classList.add("surface-window");
    void api
      .settings()
      .then((settings) => {
        document.documentElement.dataset.theme = settings.theme;
        localStorage.setItem("flux-theme", settings.theme);
        void i18n.changeLanguage(settings.locale);
      })
      .catch((error) => setError(String(error)));
    if (id)
      void api
        .task(id)
        .then(setTask)
        .catch((error) => setError(String(error)));
    const preferences = subscribe<Settings>("settings-changed", (event) => {
      document.documentElement.dataset.theme = event.payload.theme;
      localStorage.setItem("flux-theme", event.payload.theme);
      void i18n.changeLanguage(event.payload.locale);
    });
    return () => {
      void preferences.then((unlisten) => unlisten());
    };
  }, [id, i18n]);
  useEffect(() => {
    if (view !== "details" || !id) return;
    let disposed = false,
      pending = false;
    const subscription = subscribe<Progress>("download-progress", (event) => {
      if (!disposed && event.payload.id === id) setProgress(event.payload);
    });
    const timer = setInterval(async () => {
      if (pending) return;
      pending = true;
      try {
        const current = await api.task(id);
        if (!disposed) setTask(current);
      } catch (error) {
        if (!disposed) setError(String(error));
      } finally {
        pending = false;
      }
    }, 1000);
    return () => {
      disposed = true;
      clearInterval(timer);
      void subscription.then((unlisten) => unlisten());
    };
  }, [id, view]);
  useEffect(() => {
    document.documentElement.lang = i18n.language;
    document.documentElement.dir = /^(ar|ur)/.test(i18n.language)
      ? "rtl"
      : "ltr";
  }, [i18n.language]);
  if (id && !task)
    return (
      <div className="surface-loading" role="status">
        {error || t("inspecting")}
      </div>
    );
  return (
    <>
      {view === "power" && <PowerCountdown />}
      {view === "complete" && task && (
        <DownloadComplete task={task} onClose={close} />
      )}
      {view === "download" && (
        <DownloadModal
          visible
          onClose={close}
          onAdded={async () => {}}
          onError={setError}
          initialUrl={query.get("url") || ""}
          proposal={task}
        />
      )}
      {view === "settings" && (
        <SettingsModal
          visible
          onClose={close}
          onError={setError}
          onSaved={() => {}}
        />
      )}
      {view === "details" && (
        <TaskDetails
          task={task || null}
          progress={progress}
          onClose={close}
          onChanged={async () => {
            if (id) setTask(await api.task(id));
          }}
          onError={setError}
        />
      )}
    </>
  );
}
