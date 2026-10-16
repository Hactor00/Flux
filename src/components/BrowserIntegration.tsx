import { useEffect, useState } from "react";
import { Globe2, CheckCircle2, AlertCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api, desktop, subscribe, type BrowserIntegrationStatus } from "../api";
import { getVersion } from "@tauri-apps/api/app";
import { compareVersions } from "../update-version.js";

let releaseCheck: Promise<string> | undefined;
function latestRelease() {
  return (releaseCheck ??= api.latestReleaseVersion().catch((error) => {
    releaseCheck = undefined;
    throw error;
  }));
}

function useBrowserIntegration() {
  const [status, setStatus] = useState<BrowserIntegrationStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [checked, setChecked] = useState(false);
  const [release, setRelease] = useState("");
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void getVersion()
      .then((version) => {
        if (alive)
          setRelease((current) =>
            compareVersions(current, version) === 1 ? current : version,
          );
      })
      .catch(() => {});
    void latestRelease()
      .then((version) => {
        if (alive)
          setRelease((current) =>
            compareVersions(current, version) === 1 ? current : version,
          );
      })
      .catch(() => {});
    const refresh = () => {
      void api
        .browserIntegration()
        .then((value) => {
          if (alive) setStatus(value);
        })
        .catch((error) => {
          if (alive) {
            setStatus({
              registered: false,
              connected: false,
              extension_versions: [],
              extension_id: null,
              error: String(error),
              reminder_dismissed: false,
            });
          }
        });
    };
    refresh();
    const timer = setInterval(refresh, 3000);
    const subscription = subscribe<BrowserIntegrationStatus>(
      "browser-integration-changed",
      (event) => {
        if (alive) setStatus(event.payload);
      },
    );
    return () => {
      alive = false;
      clearInterval(timer);
      void subscription.then((stop) => stop());
    };
  }, []);
  async function repair() {
    setBusy(true);
    setError("");
    try {
      setStatus(await api.registerBrowser());
      for (let attempt = 0; attempt < 3; attempt++) {
        const next = await api.browserIntegration();
        setStatus(next);
        if (next.connected || !next.registered) break;
        await new Promise((resolve) => setTimeout(resolve, 1000));
      }
      setChecked(true);
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function dismiss() {
    setBusy(true);
    setError("");
    try {
      await api.dismissBrowserReminder();
      setStatus((value) =>
        value ? { ...value, reminder_dismissed: true } : value,
      );
    } catch (error) {
      setError(String(error));
    } finally {
      setBusy(false);
    }
  }
  const extensionUpdate =
    status?.extension_versions.some(
      (version) =>
        version === "unknown" || compareVersions(release, version) === 1,
    ) ?? false;
  return {
    status,
    busy,
    error,
    checked,
    release,
    extensionUpdate,
    repair,
    dismiss,
  };
}

function IntegrationState({
  status,
}: {
  status: BrowserIntegrationStatus | null;
}) {
  const { t } = useTranslation();
  return (
    <span
      className={`integration-state ${status?.connected ? "ready" : "pending"}`}
      role="status"
    >
      {status?.connected ? (
        <CheckCircle2 size={15} />
      ) : (
        <AlertCircle size={15} />
      )}
      {t(
        status
          ? status.connected
            ? "bridge_connected"
            : status.registered
              ? "bridge_disconnected"
              : "bridge_failed"
          : "bridge_checking",
      )}
    </span>
  );
}

export function BrowserIntegrationNotice() {
  const { t } = useTranslation();
  const [openError, setOpenError] = useState("");
  const [hiddenUpdate, setHiddenUpdate] = useState("");
  const {
    status,
    busy,
    error,
    checked,
    repair,
    dismiss,
    extensionUpdate,
    release,
  } = useBrowserIntegration();
  const updateKey = `${release}:${status?.extension_versions.join(",")}`;
  if (
    !desktop ||
    !status ||
    (status.reminder_dismissed &&
      (!extensionUpdate || hiddenUpdate === updateKey))
  )
    return null;
  return (
    <section
      className="integration-notice"
      aria-labelledby="integration-guide-title"
    >
      <Globe2 className="integration-icon" size={23} />
      <div className="integration-copy">
        <div className="integration-heading">
          <strong id="integration-guide-title">{t("bridge_welcome")}</strong>
          <IntegrationState status={status} />
        </div>
        <p>{t("bridge_automatic")}</p>
        <p>{t("bridge_extension_step")}</p>
        <p>{t("bridge_repair_help")}</p>
        {checked && !status.connected && (
          <p role="status">{t("bridge_not_found")}</p>
        )}
        {extensionUpdate && (
          <p role="status">
            {t("extension_update_help")} {release}
          </p>
        )}
        {openError && <p role="alert">{openError}</p>}
        {(!status.connected || extensionUpdate) && (
          <button
            className="secondary"
            onClick={() =>
              void api
                .openReleasePage()
                .catch((error) => setOpenError(String(error)))
            }
          >
            {t("release_downloads")}
          </button>
        )}
        {(error || status.error) && (
          <p className="integration-error" role="alert">
            {error || status.error}
          </p>
        )}
        {!status.connected && (
          <button
            className="secondary"
            disabled={busy}
            onClick={() => void repair()}
          >
            {t("install_bridge")}
          </button>
        )}
      </div>
      <button
        className="icon-button integration-dismiss"
        aria-label={t("bridge_dismiss")}
        title={t("bridge_dismiss")}
        disabled={busy}
        onClick={() => {
          setHiddenUpdate(updateKey);
          void dismiss();
        }}
      >
        <X size={17} />
      </button>
    </section>
  );
}

export function BrowserIntegrationSettings() {
  const { t } = useTranslation();
  const { status, busy, error, checked, release, extensionUpdate, repair } =
    useBrowserIntegration();
  const [openError, setOpenError] = useState("");
  return (
    <div className="bridge-panel">
      <Globe2 className="bridge-mark" size={32} />
      <h3>Chrome · Edge · Brave · Chromium · Firefox</h3>
      <IntegrationState status={status} />
      {status?.registered && <p className="muted">{t("bridge_ready")}</p>}
      {status && !status.connected && (
        <p role="status">
          {t(checked ? "bridge_not_found" : "bridge_connection_help")}
        </p>
      )}
      <p>{t("bridge_automatic")}</p>
      <p>{t("bridge_extension_step")}</p>
      <p>{t("bridge_repair_help")}</p>
      <button
        className="primary"
        disabled={!desktop || busy}
        onClick={() => void repair()}
      >
        {t("install_bridge")}
      </button>
      <button
        className="secondary"
        disabled={!desktop}
        onClick={() =>
          void api
            .openReleasePage()
            .catch((error) => setOpenError(String(error)))
        }
      >
        {t("release_downloads")}
      </button>
      {extensionUpdate && (
        <p role="status">
          {t("extension_update_help")} {release}
        </p>
      )}
      {status?.connected && (
        <p className="muted">
          {t("extension_versions")}: {status.extension_versions.join(", ")}
        </p>
      )}
      {openError && <p role="alert">{openError}</p>}
      {(error || status?.error) && (
        <p className="integration-error" role="alert">
          {error || status?.error}
        </p>
      )}
    </div>
  );
}
