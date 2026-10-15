import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import * as Tabs from "@radix-ui/react-tabs";
import { Check, FolderOpen, Globe2, Search, X } from "lucide-react";
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
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
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
          <Dialog.Title>{t("settings")}</Dialog.Title>
          <Dialog.Description>{t("local_first")}</Dialog.Description>
          <Dialog.Close
            className="modal-close icon-button"
            aria-label={t("close")}
          >
            <X size={18} />
          </Dialog.Close>
          <Tabs.Root defaultValue="language">
            <Tabs.List className="tabs" aria-label={t("settings")}>
              {[
                "language",
                "downloads",
                "daily_usage",
                "browserIntegration",
                "host_accounts",
                "updates",
                "seeding",
                "watch_title",
              ].map((key) => (
                <Tabs.Trigger key={key} value={key}>
                  {t(key)}
                </Tabs.Trigger>
              ))}
            </Tabs.List>
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
                <h3>{t("appearance")}</h3>
                <label>
                  {t("theme")}
                  <Choice
                    label={t("theme")}
                    value={settings.theme}
                    onChange={(value) =>
                      update("theme", value as Settings["theme"])
                    }
                    options={[
                      { value: "dark", label: t("theme_dark") },
                      { value: "light", label: t("theme_light") },
                    ]}
                  />
                </label>
              </div>
              <div className="language-heading">
                <Globe2 size={18} />
                <h3>{t("language")}</h3>
                <span>20</span>
              </div>
              <label className="search language-search">
                <Search size={16} />
                <input
                  aria-label={t("search_languages")}
                  placeholder={t("search_languages")}
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                />
              </label>
              <div className="languages">
                {languages
                  .filter((language) =>
                    `${language.name} ${language.native} ${language.code}`
                      .toLocaleLowerCase()
                      .includes(query.toLocaleLowerCase()),
                  )
                  .map((language) => (
                    <button
                      key={language.code}
                      className={
                        i18n.language === language.code ? "chosen" : ""
                      }
                      onClick={() => {
                        void i18n.changeLanguage(language.code);
                        update("locale", language.code);
                      }}
                    >
                      <span>
                        {language.native}
                        <small>{language.name}</small>
                      </span>
                      {i18n.language === language.code ? (
                        <Check size={16} />
                      ) : (
                        <span className="locale-code">{language.code}</span>
                      )}
                    </button>
                  ))}
              </div>
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
          </Tabs.Root>
          <p role="status" className="settings-notice">
            {notice}
          </p>
          <button
            className="primary"
            disabled={!desktop || busy}
            onClick={() => void save()}
          >
            {t("save")}
          </button>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
