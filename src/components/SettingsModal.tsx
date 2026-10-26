import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Tabs from "@radix-ui/react-tabs";
import {
  Check,
  Download,
  FolderOpen,
  Globe2,
  X,
  SlidersHorizontal,
  RefreshCw,
  Network,
  FolderSearch,
  Shield,
  Moon,
  Sun,
  Save,
  Monitor,
  BarChart3,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { open } from "@tauri-apps/plugin-dialog";
import { api, desktop, defaultSettings, type Settings } from "../api";
import { languages } from "../i18n";
import { Toggle, Choice } from "./primitives";
import { HostAccounts } from "./HostAccounts";
import { Updater } from "./Updater";
import { SeedingRules } from "./SeedingRules";
import { BrowserIntegrationSettings } from "./BrowserIntegration";
import { TorrentWatchSettings } from "./TorrentWatchSettings";
import { UsageStatistics } from "./UsageStatistics";
export function SettingsModal({
  visible,
  onClose,
  onError,
  onSaved,
}: {
  visible: boolean;
  onClose: (value: boolean) => void;
  onError: (error: string) => void;
  onSaved: (settings: Settings) => void;
}) {
  const { t, i18n } = useTranslation();
  const [settings, setSettings] = useState<Settings>(defaultSettings);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const [section, setSection] = useState("language");
  const sections = [
    {
      key: "language",
      title: "appearance",
      icon: <SlidersHorizontal size={17} />,
    },
    { key: "downloads", title: "downloads", icon: <Download size={17} /> },
    {
      key: "daily_usage",
      title: "daily_usage",
      icon: <SlidersHorizontal size={17} />,
    },
    {
      key: "browserIntegration",
      title: "browserIntegration",
      icon: <Globe2 size={17} />,
    },
    {
      key: "host_accounts",
      title: "host_accounts",
      icon: <Shield size={17} />,
    },
    { key: "updates", title: "updates", icon: <RefreshCw size={17} /> },
    { key: "usage", title: "usage_statistics", icon: <BarChart3 size={17} /> },
    { key: "seeding", title: "seeding", icon: <Network size={17} /> },
    {
      key: "watch_title",
      title: "watch_title",
      icon: <FolderSearch size={17} />,
    },
  ];
  useEffect(() => {
    if (visible) {
      setNotice("");
      void api
        .settings()
        .then(setSettings)
        .catch((error) => onError(String(error)));
    }
  }, [visible, onError]);
  const update = <K extends keyof Settings>(key: K, value: Settings[K]) =>
    setSettings((previous) => ({ ...previous, [key]: value }));
  async function save() {
    setBusy(true);
    try {
      const saved = await api.saveSettings({
        ...settings,
        locale: i18n.language,
      });
      setSettings(saved);
      onSaved(saved);
      setNotice(t("saved"));
    } catch (error) {
      setNotice(String(error));
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function chooseFolder() {
    try {
      const path = await open({
        directory: true,
        multiple: false,
        defaultPath: settings.download_directory ?? undefined,
      });
      if (typeof path === "string") update("download_directory", path);
    } catch (error) {
      setNotice(String(error));
      onError(String(error));
    }
  }
  return (
    <Dialog.Root open={visible} onOpenChange={onClose}>
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content className="modal settings-modal">
          <div className="settings-header">
            <span className="settings-symbol">
              <SlidersHorizontal size={22} />
            </span>
            <div>
              <Dialog.Title>{t("settings")}</Dialog.Title>
              <Dialog.Description>{t("local_first")}</Dialog.Description>
            </div>
          </div>
          <Dialog.Close
            className="modal-close icon-button"
            aria-label={t("close")}
          >
            <X size={18} />
          </Dialog.Close>
          <Tabs.Root
            className="settings-layout"
            orientation="vertical"
            value={section}
            onValueChange={setSection}
          >
            <Tabs.List
              className="settings-navigation"
              aria-label={t("settings")}
            >
              {sections.map(({ key, title, icon }) => (
                <Tabs.Trigger key={key} value={key}>
                  {icon}
                  <span>{t(title)}</span>
                </Tabs.Trigger>
              ))}
            </Tabs.List>
            <div className="settings-content">
              <h3 className="settings-section-title">
                {t(
                  sections.find((item) => item.key === section)?.title ||
                    section,
                )}
              </h3>
              <Tabs.Content value="host_accounts">
                <HostAccounts />
              </Tabs.Content>
              <Tabs.Content value="watch_title">
                <TorrentWatchSettings
                  value={settings.torrent_watch}
                  onChange={(value) => update("torrent_watch", value)}
                  onError={onError}
                />
              </Tabs.Content>
              <Tabs.Content value="updates">
                <Updater />
              </Tabs.Content>
              <Tabs.Content value="usage">
                <UsageStatistics />
              </Tabs.Content>
              <Tabs.Content value="seeding">
                <SeedingRules
                  value={settings.seeding}
                  onChange={(value) => {
                    if (value) update("seeding", value);
                  }}
                />
                <label>
                  {t("upload_limit")}
                  <input
                    type="number"
                    min={0}
                    max={2147483647}
                    value={settings.torrent_upload_limit}
                    onChange={(event) =>
                      update("torrent_upload_limit", Number(event.target.value))
                    }
                  />
                </label>
              </Tabs.Content>
              <Tabs.Content value="language">
                <div className="appearance-setting">
                  <h4>{t("theme")}</h4>
                  <div
                    className="theme-options"
                    role="group"
                    aria-label={t("theme")}
                  >
                    {(["system", "light", "dark"] as const).map((theme) => (
                      <button
                        key={theme}
                        type="button"
                        className={`theme-option ${theme}`}
                        aria-pressed={settings.theme === theme}
                        onClick={() => update("theme", theme)}
                      >
                        <span className="theme-preview" aria-hidden="true">
                          <i />
                          <span>
                            <b />
                            <b />
                            <b />
                          </span>
                        </span>
                        <span className="theme-option-label">
                          {theme === "system" ? (
                            <Monitor size={15} />
                          ) : theme === "dark" ? (
                            <Moon size={15} />
                          ) : (
                            <Sun size={15} />
                          )}
                          {t(`theme_${theme}`)}
                          {settings.theme === theme && <Check size={15} />}
                        </span>
                      </button>
                    ))}
                  </div>
                </div>
                <div className="language-setting">
                  <label>
                    <Globe2 size={16} />
                    {t("language")} <small>20</small>
                  </label>
                  <Choice
                    label={t("language")}
                    value={i18n.language}
                    onChange={(value) => {
                      void i18n.changeLanguage(value);
                      update("locale", value);
                    }}
                    options={languages.map((language) => ({
                      value: language.code,
                      label: `${language.native} · ${language.name}`,
                    }))}
                  />
                </div>
                <Toggle
                  checked={settings.compact_list}
                  onChange={(value) => update("compact_list", value)}
                  label={t("compact_list")}
                />
              </Tabs.Content>
              <Tabs.Content value="downloads">
                <div className="form-grid settings-fields">
                  <label>
                    {t("concurrent_tasks")}
                    <input
                      type="number"
                      min={1}
                      max={8}
                      value={settings.concurrent_tasks}
                      onChange={(event) =>
                        update("concurrent_tasks", Number(event.target.value))
                      }
                    />
                  </label>
                  <label>
                    {t("connections")}
                    <Choice
                      label={t("connections")}
                      value={String(settings.connections)}
                      onChange={(value) => update("connections", Number(value))}
                      options={[
                        { value: "0", label: t("automatic") },
                        ...Array.from({ length: 32 }, (_, index) => ({
                          value: String(index + 1),
                          label: String(index + 1),
                        })),
                      ]}
                    />
                  </label>
                </div>
                <label>
                  {t("global_speed_limit")}
                  <input
                    type="number"
                    min={0}
                    value={settings.speed_limit}
                    onChange={(event) =>
                      update("speed_limit", Number(event.target.value))
                    }
                  />
                </label>
                <label>
                  {t("download_directory")}
                  <div className="folder-field">
                    <input readOnly value={settings.download_directory ?? ""} />
                    <button
                      onClick={() => void chooseFolder()}
                      disabled={!desktop}
                      aria-label={t("choose_folder")}
                    >
                      <FolderOpen size={17} />
                    </button>
                  </div>
                </label>
                <div className="settings-toggles">
                  {(
                    [
                      "queue_enabled",
                      "clipboard_monitor",
                      "minimize_to_tray",
                      "notifications",
                    ] as const
                  ).map((key) => (
                    <Toggle
                      key={key}
                      label={t(key)}
                      checked={settings[key]}
                      onChange={(value) => update(key, value)}
                    />
                  ))}
                </div>
              </Tabs.Content>
              <Tabs.Content value="daily_usage">
                <Toggle
                  label={t("completion_enabled")}
                  checked={settings.completion_action !== "none"}
                  onChange={(value) =>
                    update("completion_action", value ? "exit_app" : "none")
                  }
                />
                <label>
                  {t("completion_action")}
                  <Choice
                    label={t("completion_action")}
                    disabled={settings.completion_action === "none"}
                    value={
                      settings.completion_action === "none"
                        ? "exit_app"
                        : settings.completion_action
                    }
                    onChange={(value) =>
                      update(
                        "completion_action",
                        value as Settings["completion_action"],
                      )
                    }
                    options={[
                      "exit_app",
                      "sleep",
                      "hibernate",
                      "shutdown_computer",
                    ].map((value) => ({ value, label: t(`action_${value}`) }))}
                  />
                </label>
                <p className="field-hint">{t("power_hint")}</p>
                <Toggle
                  label={t("auto_categorize")}
                  checked={settings.auto_categorize}
                  onChange={(value) => update("auto_categorize", value)}
                />
                {settings.auto_categorize && (
                  <div className="category-folders">
                    {(
                      [
                        "compressed",
                        "video",
                        "music",
                        "programs",
                        "documents",
                      ] as const
                    ).map((key) => (
                      <label key={key}>
                        {t(`category_${key}`)}
                        <div className="folder-field">
                          <input
                            readOnly
                            value={
                              settings.category_folders[key] || t("automatic")
                            }
                          />
                          <button
                            aria-label={t("choose_folder")}
                            onClick={async () => {
                              try {
                                const path = await open({
                                  directory: true,
                                  multiple: false,
                                  defaultPath:
                                    settings.category_folders[key] || undefined,
                                });
                                if (typeof path === "string")
                                  setSettings((previous) => ({
                                    ...previous,
                                    category_folders: {
                                      ...previous.category_folders,
                                      [key]: path,
                                    },
                                  }));
                              } catch (e) {
                                onError(String(e));
                              }
                            }}
                          >
                            <FolderOpen size={17} />
                          </button>
                          <button
                            className="secondary"
                            onClick={() =>
                              setSettings((previous) => ({
                                ...previous,
                                category_folders: {
                                  ...previous.category_folders,
                                  [key]: null,
                                },
                              }))
                            }
                          >
                            {t("reset")}
                          </button>
                        </div>
                      </label>
                    ))}
                  </div>
                )}
                <label>
                  {t("clipboard_mode")}
                  <Choice
                    label={t("clipboard_mode")}
                    value={settings.clipboard_mode}
                    onChange={(value) =>
                      update(
                        "clipboard_mode",
                        value as Settings["clipboard_mode"],
                      )
                    }
                    options={[
                      { value: "approval", label: t("clipboard_approval") },
                      { value: "silent_queue", label: t("clipboard_silent") },
                    ]}
                  />
                </label>
                <Toggle
                  label={t("launch_at_startup")}
                  checked={settings.launch_at_startup}
                  onChange={(value) => update("launch_at_startup", value)}
                />
              </Tabs.Content>
              <Tabs.Content value="browserIntegration">
                <BrowserIntegrationSettings />
              </Tabs.Content>
            </div>
          </Tabs.Root>
          <div className="settings-footer">
            <p role="status" className="settings-notice">
              {notice}
            </p>
            <button
              className="primary"
              disabled={!desktop || busy}
              onClick={() => void save()}
            >
              <Save size={16} />
              {t("save")}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
