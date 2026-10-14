import { useEffect, useState } from "react";
import { Globe2, CheckCircle2, AlertCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { api, desktop, subscribe, type BrowserIntegrationStatus } from "../api";

function useBrowserIntegration() {
  const [status, setStatus] = useState<BrowserIntegrationStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    if (!desktop) return;
    let alive = true;
    void api
      .browserIntegration()
      .then((value) => {
        if (alive) setStatus(value);
      })
      .catch((error) => {
        if (alive) {
          setStatus({
            registered: false,
            extension_id: null,
            error: String(error),
            reminder_dismissed: false,
          });
        }
      });
    const subscription = subscribe<BrowserIntegrationStatus>(
      "browser-integration-changed",
      (event) => {
        if (alive) setStatus(event.payload);
      },
    );
    return () => {
      alive = false;
      void subscription.then((stop) => stop());
    };
  }, []);
  async function repair() {
    setBusy(true);
    setError("");
    try {
      await api.registerBrowser();
      setStatus(await api.browserIntegration());
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
  return { status, busy, error, repair, dismiss };
}

function IntegrationState({
  status,
}: {
  status: BrowserIntegrationStatus | null;
}) {
  const { t } = useTranslation();
  return (
    <span
      className={`integration-state ${status?.registered ? "ready" : "pending"}`}
      role="status"
    >
      {status?.registered ? (
        <CheckCircle2 size={15} />
      ) : (
        <AlertCircle size={15} />
      )}
      {t(
        status
          ? status.registered
            ? "bridge_ready"
            : "bridge_failed"
          : "bridge_checking",
      )}
    </span>
  );
}

export function BrowserIntegrationNotice() {
  const { t } = useTranslation();
  const { status, busy, error, repair, dismiss } = useBrowserIntegration();
  if (!desktop || !status || status.reminder_dismissed) return null;
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
        {(error || status.error) && (
          <p className="integration-error" role="alert">
            {error || status.error}
          </p>
        )}
        {!status.registered && (
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
        onClick={() => void dismiss()}
      >
        <X size={17} />
      </button>
    </section>
  );
}

export function BrowserIntegrationSettings() {
  const { t } = useTranslation();
  const { status, busy, error, repair } = useBrowserIntegration();
  return (
    <div className="bridge-panel">
      <Globe2 className="bridge-mark" size={32} />
      <h3>Chrome · Edge · Brave · Chromium · Firefox</h3>
      <IntegrationState status={status} />
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
      {(error || status?.error) && (
        <p className="integration-error" role="alert">
          {error || status?.error}
        </p>
      )}
    </div>
  );
}
