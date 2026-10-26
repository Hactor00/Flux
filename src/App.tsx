import { useCallback, useEffect, useState, useRef, useMemo } from "react";
import {
  Plus,
  Play,
  Pause,
  Search,
  Settings2,
  Download,
  Check,
  FolderOpen,
  Trash2,
  Info,
  ArrowDown,
  Clock,
  AlertCircle,
  ChevronDown,
  File,
  Film,
  Music,
  Archive,
  Network,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import packageInfo from "../package.json";
import { isTorrentTask, canPauseTask, canResumeTask } from "./downloads";
import * as Menu from "@radix-ui/react-dropdown-menu";
import * as Context from "@radix-ui/react-context-menu";
import * as Alert from "@radix-ui/react-alert-dialog";
import { useVirtualizer } from "@tanstack/react-virtual";
import {
  api,
  desktop,
  taskFilename,
  taskTime,
  subscribe,
  type Progress,
  type Task,
  type Settings,
} from "./api";
import { DownloadModal } from "./components/DownloadModal";
import { SettingsModal } from "./components/SettingsModal";
import { TaskDetails } from "./components/TaskDetails";
import { SpeedGraph } from "./components/SpeedGraph";
import { BrowserIntegrationNotice } from "./components/BrowserIntegration";
import "./desktop.css";
import { transferView, formatDuration } from "./transfer";
import { applyTheme, storedTheme, type Theme } from "./theme";

function bytes(value: number, locale: string) {
  const units = ["B", "KB", "MB", "GB", "TB"];
  const exponent = Math.min(
    4,
    Math.max(0, Math.floor(Math.log2(Math.max(1, value)) / 10)),
  );
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: 1 }).format(value / 1024 ** exponent)} ${units[exponent]}`;
}
function FileIcon({ task }: { task: Task }) {
  if (isTorrentTask(task)) return <Network size={16} />;
  const name = taskFilename(task);
  return /\.(mp4|mkv|webm|mov)$/i.test(name) ? (
    <Film size={16} />
  ) : /\.(mp3|m4a|aac|flac|wav)$/i.test(name) ? (
    <Music size={16} />
  ) : /\.(zip|rar|7z|tar|gz)$/i.test(name) ? (
    <Archive size={16} />
  ) : (
    <File size={16} />
  );
}
export default function App() {
  const { t, i18n } = useTranslation();
  const dateFormatter = useMemo(
    () =>
      new Intl.DateTimeFormat(i18n.language, {
        dateStyle: "short",
        timeStyle: "short",
      }),
    [i18n.language],
  );
  const [tasks, setTasks] = useState<Task[]>([]),
    [live, setLive] = useState<Record<string, Progress>>({}),
    [error, setError] = useState(""),
    [filter, setFilter] = useState("all"),
    [search, setSearch] = useState(""),
    [selected, setSelected] = useState<Set<string>>(new Set()),
    [sort, setSort] = useState<
      "file" | "status" | "total" | "download_time" | null
    >(null),
    [ascending, setAscending] = useState(true);
  const [version, setVersion] = useState(packageInfo.version);
  const [theme, setTheme] = useState<Theme>(storedTheme);
  const [compactList, setCompactList] = useState(false);
  const rowHeight = compactList ? 42 : 58;
  useEffect(() => {
    if (isTauri())
      void getVersion()
        .then(setVersion)
        .catch(() => {});
  }, []);
  const [addOpen, setAddOpen] = useState(false),
    [settingsOpen, setSettingsOpen] = useState(false),
    [details, setDetails] = useState<Task | null>(null),
    [removeOpen, setRemoveOpen] = useState(false);
  const columns = useRef<HTMLDivElement>(null);
  const refresh = useCallback(async () => {
    setTasks(await api.list());
  }, []);
  const run = useCallback(
    async (operation: () => Promise<unknown>) => {
      try {
        setError("");
        await operation();
        await refresh();
      } catch (error) {
        setError(String(error));
      }
    },
    [refresh],
  );
  const openAdd = useCallback(() => {
    if (isTauri()) void run(() => api.surface("download"));
    else setAddOpen(true);
  }, [run]);
  const openSettings = () => {
    if (isTauri()) void run(() => api.surface("settings"));
    else setSettingsOpen(true);
  };
  const openDetails = (task: Task) => {
    if (isTauri())
      void run(() =>
        api.surface(
          task.status === "awaiting_approval" ? "download" : "details",
          task.id,
        ),
      );
    else setDetails(task);
  };
  useEffect(() => {
    let alive = true,
      busy = false;
    const update = async () => {
      if (busy) return;
      busy = true;
      try {
        const items = await api.list();
        if (alive) setTasks(items);
      } catch (error) {
        if (alive) setError(String(error));
      } finally {
        busy = false;
      }
    };
    void update();
    const timer = setInterval(() => void update(), 1500);
    void api
      .settings()
      .then((s) => {
        setTheme(s.theme);
        setCompactList(s.compact_list);
        void i18n.changeLanguage(s.locale);
      })
      .catch((e) => setError(String(e)));
    const subscription = subscribe<Progress>("download-progress", (event) => {
      if (alive)
        setLive((previous) => ({
          ...previous,
          [event.payload.id]: event.payload,
        }));
    });
    const preferences = subscribe<Settings>("settings-changed", (event) => {
      if (alive) {
        setTheme(event.payload.theme);
        setCompactList(event.payload.compact_list);
        void i18n.changeLanguage(event.payload.locale);
      }
    });
    const failures = subscribe<string>("background-error", (event) => {
      if (alive) setError(event.payload);
    });
    return () => {
      alive = false;
      clearInterval(timer);
      void subscription.then((stop) => stop());
      void preferences.then((stop) => stop());
      void failures.then((stop) => stop());
    };
  }, [i18n]);
  useEffect(() => {
    applyTheme(theme);
  }, [theme]);
  useEffect(() => {
    document.documentElement.lang = i18n.language;
    document.documentElement.dir = /^(ar|ur)/.test(i18n.language)
      ? "rtl"
      : "ltr";
    localStorage.setItem("flux-language", i18n.language);
  }, [i18n.language]);
  const shown = useMemo(() => {
    const items = tasks.filter(
      (task) =>
        task.status !== "cancelled" &&
        taskFilename(task)
          .toLocaleLowerCase()
          .includes(search.toLocaleLowerCase()) &&
        (filter === "all" ||
          (filter === "torrent" && isTorrentTask(task)) ||
          (filter === "seeding" && task.torrent?.state === "seeding") ||
          (filter === "active" &&
            ["running", "queued", "verifying"].includes(task.status)) ||
          task.status === filter),
    );
    if (sort)
      items.sort((a, b) => {
        const direction = ascending ? 1 : -1;
        return (
          direction *
          (sort === "total"
            ? (a.total || 0) - (b.total || 0)
            : sort === "download_time"
              ? (taskTime(a) || 0) - (taskTime(b) || 0)
              : sort === "file"
                ? taskFilename(a).localeCompare(taskFilename(b))
                : a.status.localeCompare(b.status))
        );
      });
    return items;
  }, [tasks, search, filter, sort, ascending]);
  const active = tasks.filter((task) =>
    ["running", "verifying"].includes(task.status),
  );
  const speed = active.reduce(
    (sum, task) => sum + (live[task.id]?.bytes_per_second || 0),
    0,
  );
  const selectedTasks = shown.filter((task) => selected.has(task.id));
  const canPause = selectedTasks.some(canPauseTask);
  const canResume = selectedTasks.some(canResumeTask);
  const canStartQueue = tasks.some((task) =>
    ["queued", "paused"].includes(task.status),
  );
  const canPauseAll = tasks.some(canPauseTask);
  useEffect(() => {
    setSelected(new Set());
  }, [filter, search]);
  const selectedTransfer = async (action: "pause" | "resume") => {
    for (const task of selectedTasks)
      if ((action === "pause" ? canPauseTask : canResumeTask)(task))
        await api[action](task.id);
  };
  useEffect(() => {
    const key = (event: KeyboardEvent) => {
      if (
        event.key === "Escape" &&
        !document.querySelector(
          '[role="dialog"], [role="alertdialog"], [role="menu"]',
        )
      ) {
        setSelected(new Set());
        return;
      }
      if (
        (event.target as HTMLElement).closest('input,textarea,[role="dialog"]')
      )
        return;
      if (event.ctrlKey && event.key.toLowerCase() === "n") {
        event.preventDefault();
        openAdd();
      } else if (event.key === "Delete" && selected.size) setRemoveOpen(true);
      else if (event.key === " " && selected.size) {
        event.preventDefault();
        void run(() => selectedTransfer(canPause ? "pause" : "resume"));
      } else if (event.ctrlKey && event.key.toLowerCase() === "a") {
        event.preventDefault();
        setSelected(new Set(shown.map((task) => task.id)));
      }
    };
    addEventListener("keydown", key);
    return () => removeEventListener("keydown", key);
  });
  const viewport = useRef<HTMLDivElement>(null),
    lastIndex = useRef(0);
  const itemKey = useCallback((index: number) => shown[index].id, [shown]);
  const virtualizer = useVirtualizer({
    count: shown.length,
    getScrollElement: () => viewport.current,
    estimateSize: () => rowHeight,
    overscan: 8,
    getItemKey: itemKey,
  });
  useEffect(() => {
    virtualizer.measure();
  }, [rowHeight, virtualizer]);
  function select(
    task: Task,
    index: number,
    event: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean },
  ) {
    setSelected((previous) => {
      if (event.shiftKey)
        return new Set(
          shown
            .slice(
              Math.min(index, lastIndex.current),
              Math.max(index, lastIndex.current) + 1,
            )
            .map((task) => task.id),
        );
      lastIndex.current = index;
      if (event.ctrlKey || event.metaKey) {
        const next = new Set(previous);
        if (next.has(task.id)) next.delete(task.id);
        else next.add(task.id);
        return next;
      }
      return new Set([task.id]);
    });
  }
  const count = (key: string) =>
    tasks.filter((task) =>
      key === "all"
        ? task.status !== "cancelled"
        : key === "torrent"
          ? task.status !== "cancelled" && isTorrentTask(task)
          : key === "active"
            ? ["running", "verifying", "queued"].includes(task.status)
            : key === "seeding"
              ? task.torrent?.state === "seeding"
              : task.status === key,
    ).length;
  const menuItem = (text: string, action: () => void, disabled = false) => (
    <Menu.Item
      className="desktop-menu-item"
      disabled={disabled}
      onSelect={action}
    >
      {t(text)}
    </Menu.Item>
  );
  return (
    <div
      className={`desktop-shell ${compactList ? "compact-list" : ""}`}
      onPointerDownCapture={(event) => {
        if (
          !(event.target as HTMLElement).closest(
            '.desktop-row,.desktop-toolbar,.desktop-inspector,.desktop-menu,[role="dialog"],[role="alertdialog"],[role="menu"]',
          )
        )
          setSelected(new Set());
      }}
    >
      <div className="desktop-menu">
        <span className="desktop-logo">
          <Download size={16} />
          Flux
        </span>
        <Menu.Root>
          <Menu.Trigger>
            {t("file")}
            <ChevronDown size={11} />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Content className="desktop-menu-content" sideOffset={4}>
              {menuItem("new_download", openAdd, !desktop)}
              {menuItem("batch", openAdd, !desktop)}
              <Menu.Separator />
              {menuItem("settings", openSettings)}
            </Menu.Content>
          </Menu.Portal>
        </Menu.Root>
        <Menu.Root>
          <Menu.Trigger>
            {t("transfer_queue")}
            <ChevronDown size={11} />
          </Menu.Trigger>
          <Menu.Portal>
            <Menu.Content className="desktop-menu-content" sideOffset={4}>
              {menuItem(
                "queue_start",
                () => void run(api.startQueue),
                !desktop || !canStartQueue,
              )}
              {menuItem(
                "pause_all",
                () => void run(api.pauseAll),
                !desktop || !canPauseAll,
              )}
            </Menu.Content>
          </Menu.Portal>
        </Menu.Root>
        <button className="desktop-menu-action" onClick={openSettings}>
          {t("settings")}
        </button>
        <span className="desktop-version">{version}</span>
      </div>
      <div className="desktop-toolbar">
        <button className="primary" onClick={openAdd} disabled={!desktop}>
          <Plus size={17} />
          {t("new_download")}
        </button>
        <span className="toolbar-divider" />
        <button
          onClick={() => void run(() => selectedTransfer("resume"))}
          disabled={!desktop || !canResume}
        >
          <Play size={17} />
          {t("resume")}
        </button>
        <button
          onClick={() => void run(() => selectedTransfer("pause"))}
          disabled={!desktop || !canPause}
        >
          <Pause size={17} />
          {t("pause")}
        </button>
        <button
          onClick={() => setRemoveOpen(true)}
          disabled={!desktop || !selectedTasks.length}
        >
          <Trash2 size={17} />
          {t("remove")}
        </button>
        <span className="toolbar-divider" />
        <button
          onClick={() => void run(api.startQueue)}
          disabled={!desktop || !canStartQueue}
        >
          <Clock size={17} />
          {t("queue_start")}
        </button>
        <button
          onClick={() => void run(api.pauseAll)}
          disabled={!desktop || !canPauseAll}
        >
          <Pause size={17} />
          {t("pause_all")}
        </button>
        <div className="toolbar-spacer" />
        <button onClick={openSettings} aria-label={t("settings")}>
          <Settings2 size={18} />
        </button>
      </div>
      <BrowserIntegrationNotice />
      <div className="desktop-workspace">
        <aside className="desktop-sidebar">
          <span className="sidebar-caption">{t("downloads")}</span>
          {[
            "all",
            "active",
            "complete",
            "paused",
            "failed",
            "awaiting_approval",
            "torrent",
            "seeding",
          ].map((key, index) => (
            <button
              key={key}
              aria-label={t(key)}
              title={t(key)}
              className={filter === key ? "selected" : ""}
              onClick={() => setFilter(key)}
            >
              {
                [
                  <Download size={16} />,
                  <ArrowDown size={16} />,
                  <Check size={16} />,
                  <Pause size={16} />,
                  <AlertCircle size={16} />,
                  <Clock size={16} />,
                  <Network size={16} />,
                  <Network size={16} />,
                ][index]
              }
              <span>{t(key)}</span>
              <small>{count(key)}</small>
            </button>
          ))}
          <div className="sidebar-network">
            <span className="status-dot online" />
            {t(desktop ? "engine_ready" : "preview")}
            <SpeedGraph speed={speed} />
            <strong>{bytes(speed, i18n.language)}/s</strong>
          </div>
        </aside>
        <main className="desktop-main">
          <div className="desktop-list-toolbar">
            <strong>
              {t(filter)}
              <small>{shown.length}</small>
            </strong>
            <label className="search">
              <Search size={14} />
              <input
                value={search}
                onChange={(event) => setSearch(event.target.value)}
                placeholder={t("search_downloads")}
                aria-label={t("search_downloads")}
              />
            </label>
          </div>
          {error && (
            <div className="desktop-error" role="alert">
              {t("action_failed")}
              <details>
                <summary>{t("details")}</summary>
                {error}
              </details>
              <button onClick={() => setError("")} aria-label={t("close")}>
                ×
              </button>
            </div>
          )}
          <div
            className="desktop-table"
            role="table"
            aria-label={t("downloads")}
          >
            <div ref={columns} className="desktop-columns" role="row">
              {[
                "file",
                "size",
                "progress",
                "status",
                "network_speed",
                "remaining",
                "download_time",
              ].map((key) => (
                <button
                  key={key}
                  role="columnheader"
                  onClick={() => {
                    const field =
                      key === "size"
                        ? "total"
                        : key === "file"
                          ? "file"
                          : key === "status"
                            ? "status"
                            : key === "download_time"
                              ? "download_time"
                              : null;
                    if (field) {
                      setSort(field);
                      setAscending(sort === field ? !ascending : true);
                    }
                  }}
                >
                  {t(key)}
                  {(sort === key || (sort === "total" && key === "size")) && (
                    <span>{ascending ? "↑" : "↓"}</span>
                  )}
                </button>
              ))}
            </div>
            <div
              ref={viewport}
              className="desktop-table-scroll"
              role="rowgroup"
              onScroll={(event) => {
                if (columns.current)
                  columns.current.style.transform = `translateX(${-event.currentTarget.scrollLeft}px)`;
              }}
            >
              <div
                style={{
                  height: virtualizer.getTotalSize(),
                  position: "relative",
                }}
              >
                {virtualizer.getVirtualItems().map((row) => {
                  const task = shown[row.index],
                    progress =
                      task.status === "running" ? live[task.id] : undefined,
                    view = transferView(task, progress),
                    { total, rate, percent, eta: remaining } = view;
                  return (
                    <Context.Root key={task.id}>
                      <Context.Trigger asChild>
                        <div
                          role="row"
                          aria-selected={selected.has(task.id)}
                          className={`desktop-row ${selected.has(task.id) ? "selected" : ""}`}
                          onClick={(event) => select(task, row.index, event)}
                          onDoubleClick={() => openDetails(task)}
                          draggable={isTauri() && task.status === "complete"}
                          onDragStart={(event) => {
                            event.preventDefault();
                            const ids = selected.has(task.id)
                              ? tasks
                                  .filter(
                                    (item) =>
                                      selected.has(item.id) &&
                                      item.status === "complete",
                                  )
                                  .map((item) => item.id)
                              : [task.id];
                            void api
                              .dragCompleted(ids)
                              .catch((error) => setError(String(error)));
                          }}
                          onContextMenu={() => {
                            if (!selected.has(task.id))
                              setSelected(new Set([task.id]));
                          }}
                          style={{
                            position: "absolute",
                            width: "100%",
                            height: rowHeight,
                            transform: `translateY(${row.start}px)`,
                          }}
                        >
                          <div role="cell" className="desktop-file">
                            <FileIcon task={task} />
                            <div className="desktop-file-copy">
                              <span title={taskFilename(task)}>
                                {taskFilename(task)}
                              </span>
                              <small>
                                {total == null
                                  ? "—"
                                  : `${view.estimated ? "≈ " : ""}${bytes(total, i18n.language)}`}
                              </small>
                            </div>
                          </div>
                          <span role="cell">
                            {total == null
                              ? "—"
                              : `${view.estimated ? "≈ " : ""}${bytes(total, i18n.language)}`}
                          </span>
                          <div role="cell" className="desktop-progress">
                            <div
                              className={`progress-track ${percent == null ? "indeterminate" : ""}`}
                              role="progressbar"
                              aria-label={taskFilename(task)}
                              aria-valuenow={
                                percent == null
                                  ? undefined
                                  : Math.floor(percent)
                              }
                              aria-valuemin={0}
                              aria-valuemax={100}
                            >
                              <div
                                style={{
                                  transform:
                                    percent == null
                                      ? undefined
                                      : `scaleX(${percent / 100})`,
                                }}
                              />
                            </div>
                            <small>
                              {percent == null
                                ? "—"
                                : `${view.percentEstimated ? "≈ " : ""}${percent.toFixed(1)}%`}
                            </small>
                          </div>
                          <span
                            role="cell"
                            className={`desktop-state ${task.status}`}
                            title={task.error || ""}
                          >
                            {t(
                              view.phase === "processing"
                                ? "media_finalizing"
                                : view.phase === "live"
                                  ? "live_stream"
                                  : task.torrent?.state === "seeding"
                                    ? "seeding"
                                    : task.status,
                            )}
                          </span>
                          <span role="cell">
                            {rate ? `${bytes(rate, i18n.language)}/s` : "—"}
                          </span>
                          <span role="cell">
                            {view.phase === "processing"
                              ? t("media_finalizing")
                              : view.phase === "live"
                                ? t("live_stream")
                                : remaining == null
                                  ? "—"
                                  : `≈ ${formatDuration(remaining)}`}
                          </span>
                          <span
                            role="cell"
                            className="desktop-date"
                            title={
                              task.timestamp_estimated
                                ? t("estimated_time")
                                : undefined
                            }
                          >
                            {taskTime(task) ? (
                              <time
                                dateTime={new Date(
                                  taskTime(task)! * 1000,
                                ).toISOString()}
                              >
                                {task.timestamp_estimated ? "≈ " : ""}
                                {dateFormatter.format(taskTime(task)! * 1000)}
                              </time>
                            ) : (
                              "—"
                            )}
                          </span>
                        </div>
                      </Context.Trigger>
                      <Context.Portal>
                        <Context.Content className="desktop-menu-content">
                          <Context.Item
                            className="desktop-menu-item"
                            onSelect={() => openDetails(task)}
                          >
                            <Info size={14} />
                            {t("details")}
                          </Context.Item>
                          <Context.Item
                            className="desktop-menu-item"
                            disabled={
                              !desktop ||
                              !(canPauseTask(task) || canResumeTask(task))
                            }
                            onSelect={() =>
                              void run(() =>
                                api[canPauseTask(task) ? "pause" : "resume"](
                                  task.id,
                                ),
                              )
                            }
                          >
                            {t(canPauseTask(task) ? "pause" : "resume")}
                          </Context.Item>
                          <Context.Item
                            className="desktop-menu-item"
                            disabled={task.status !== "complete"}
                            onSelect={() => void run(() => api.open(task.id))}
                          >
                            {t(
                              isTorrentTask(task) ? "open_folder" : "open_file",
                            )}
                          </Context.Item>
                          <Context.Item
                            className="desktop-menu-item"
                            disabled={task.status !== "complete"}
                            onSelect={() =>
                              void run(() => api.open(task.id, true))
                            }
                          >
                            <FolderOpen size={14} />
                            {t("reveal_file")}
                          </Context.Item>
                          <Context.Separator />
                          <Context.Item
                            className="desktop-menu-item danger"
                            onSelect={() => setRemoveOpen(true)}
                          >
                            {t("remove")}
                          </Context.Item>
                        </Context.Content>
                      </Context.Portal>
                    </Context.Root>
                  );
                })}
              </div>
              {!shown.length && (
                <div className="desktop-empty">
                  <Download size={32} />
                  <strong>{t(search ? "no_results" : "empty_title")}</strong>
                  <span>{t(search ? "search_hint" : "empty_body")}</span>
                  <button
                    className="secondary"
                    onClick={openAdd}
                    disabled={!desktop}
                  >
                    {t("new_download")}
                    <kbd>Ctrl N</kbd>
                  </button>
                </div>
              )}
            </div>
          </div>
          {selectedTasks.length > 0 && (
            <div className="desktop-inspector">
              {selectedTasks.length === 1 ? (
                <>
                  <FileIcon task={selectedTasks[0]} />
                  <div>
                    <strong>{taskFilename(selectedTasks[0])}</strong>
                    <span title={selectedTasks[0].destination}>
                      {selectedTasks[0].error ||
                        selectedTasks[0].destination.replace(/^\\\\\?\\/, "")}
                    </span>
                  </div>
                  <button
                    className="secondary"
                    onClick={() => openDetails(selectedTasks[0])}
                  >
                    <Info size={14} />
                    {t("details")}
                  </button>
                </>
              ) : (
                <span>
                  {t("transfer_queue")} · {shown.length} {t("file")}
                </span>
              )}
            </div>
          )}
        </main>
      </div>
      <footer className="desktop-status">
        <span>
          <span className="status-dot online" />
          {t("active_transfers")}: {active.length}
        </span>
        <span>
          {t("network_speed")}: {bytes(speed, i18n.language)}/s
        </span>
        <span>
          {t("complete")}: {count("complete")}
        </span>
        <span className="toolbar-spacer" />
        <span>{t("local_first")}</span>
      </footer>
      <DownloadModal
        visible={addOpen}
        onClose={setAddOpen}
        onAdded={refresh}
        onError={setError}
      />
      <SettingsModal
        visible={settingsOpen}
        onClose={setSettingsOpen}
        onError={setError}
        onSaved={(settings) => {
          setTheme(settings.theme);
          setCompactList(settings.compact_list);
        }}
      />
      <TaskDetails
        task={details}
        onClose={() => setDetails(null)}
        onChanged={refresh}
        onError={setError}
      />
      <Alert.Root open={removeOpen} onOpenChange={setRemoveOpen}>
        <Alert.Portal>
          <Alert.Overlay className="modal-overlay" />
          <Alert.Content className="modal confirm-modal">
            <Alert.Title>{t("remove")}</Alert.Title>
            <Alert.Description>{t("remove_description")}</Alert.Description>
            <div className="confirm-actions">
              <Alert.Cancel className="secondary">{t("close")}</Alert.Cancel>
              <Alert.Action
                className="danger-button"
                onClick={() =>
                  void run(async () => {
                    for (const task of selectedTasks) await api.remove(task.id);
                    setSelected(new Set());
                  })
                }
              >
                {t("confirm")}
              </Alert.Action>
            </div>
          </Alert.Content>
        </Alert.Portal>
      </Alert.Root>
    </div>
  );
}
