import { useEffect, useRef, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { BarChart3, Shield, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  api,
  desktop,
  subscribe,
  type UsageChoice,
  type UsageStatus,
} from "../api";
import { Toggle } from "./primitives";

function UsagePrivacy({ expanded = false }: { expanded?: boolean }) {
  const { t } = useTranslation();
  return (
    <details className="usage-privacy" open={expanded}>
      <summary>
        <Shield size={14} />
        {t("usage_privacy")}
      </summary>
      <p>{t("usage_privacy_text")}</p>
      <p>Flux · Rıza Altıner · rizaa.altiner@gmail.com</p>
      <button
        type="button"
        className="link-button"
        onClick={() => void api.openUsagePrivacy()}
      >
        fluxdownload.org/privacy
      </button>
    </details>
  );
}

export function UsageStatistics({ startup = false }: { startup?: boolean }) {
  const { t, i18n } = useTranslation();
  const [status, setStatus] = useState<UsageStatus | null>(null);
  const [visible, setVisible] = useState(false);
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const inFlight = useRef(false);
  useEffect(() => {
    let alive = true;
    const refresh = async () => {
      if (
        inFlight.current ||
        (startup && (document.hidden || !document.hasFocus()))
      )
        return;
      inFlight.current = true;
      try {
        const value = await api.statisticsStatus();
        if (!alive) return;
        setStatus(value);
        if (
          startup &&
          desktop &&
          value.prompt_due &&
          !document.querySelector('[role="dialog"]')
        )
          setVisible(true);
        if (startup && desktop && (value.enabled || value.deletion_pending)) {
          void api
            .statisticsTick()
            .then((next) => {
              if (alive) setStatus(next);
            })
            .catch(() => {});
        }
      } catch (error) {
        if (!startup && alive) setNotice(String(error));
      } finally {
        inFlight.current = false;
      }
    };
    const initial = setTimeout(() => void refresh(), startup ? 5000 : 0);
    const interval = setInterval(
      () => void refresh(),
      startup ? 30_000 : 60_000,
    );
    const focus = () => {
      void refresh();
    };
    window.addEventListener("focus", focus);
    const changed = subscribe<UsageStatus>(
      "usage-preferences-changed",
      (event) => {
        if (alive) {
          setStatus(event.payload);
          if (!event.payload.prompt_due) setVisible(false);
        }
      },
    );
    return () => {
      alive = false;
      clearTimeout(initial);
      clearInterval(interval);
      window.removeEventListener("focus", focus);
      void changed.then((stop) => stop());
    };
  }, [startup]);
  async function choose(choice: UsageChoice) {
    if (busy || !desktop) return;
    setBusy(true);
    setNotice("");
    try {
      setStatus(await api.statisticsChoice(choice));
      setVisible(false);
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(false);
    }
  }
  if (!startup)
    return (
      <section className="usage-settings">
        <p className="muted">{t("usage_help")}</p>
        <Toggle
          label={t("usage_enable")}
          checked={status?.enabled ?? false}
          onChange={(value) => {
            if (!busy) void choose(value ? "enabled" : "never");
          }}
        />
        {!status?.enabled && (
          <Toggle
            label={t("usage_reminders")}
            checked={status?.choice === "never"}
            onChange={(value) => {
              if (!busy) void choose(value ? "never" : "later");
            }}
          />
        )}
        {status?.enabled && (
          <p className="muted">
            {status.last_success
              ? `${t("usage_last_success")}: ${new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }).format(status.last_success * 1000)}`
              : t("usage_pending")}
          </p>
        )}
        {status?.deletion_pending && (
          <p role="status">{t("usage_deletion_pending")}</p>
        )}
        <UsagePrivacy />
        {notice && (
          <p className="form-error" role="alert">
            {notice}
          </p>
        )}
      </section>
    );
  return (
    <Dialog.Root
      open={visible}
      onOpenChange={(value) => {
        if (!value && !busy) void choose("later");
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content
          className="modal usage-modal"
          onEscapeKeyDown={(event) => {
            if (busy) event.preventDefault();
          }}
          onPointerDownOutside={(event) => {
            if (busy) event.preventDefault();
          }}
        >
          <span className="usage-symbol">
            <BarChart3 size={23} />
          </span>
          <Dialog.Title>{t("usage_title")}</Dialog.Title>
          <Dialog.Description>{t("usage_help")}</Dialog.Description>
          <Dialog.Close
            className="modal-close icon-button"
            aria-label={t("close")}
            disabled={busy}
          >
            <X size={17} />
          </Dialog.Close>
          <UsagePrivacy expanded />
          {notice && (
            <p className="form-error" role="alert">
              {notice}
            </p>
          )}
          <div className="dialog-actions usage-actions">
            <button
              className="secondary"
              disabled={busy}
              onClick={() => void choose("declined")}
            >
              {t("usage_decline")}
            </button>
            <button
              className="secondary"
              disabled={busy}
              onClick={() => void choose("later")}
            >
              {t("usage_later")}
            </button>
            <button
              className="primary"
              disabled={busy}
              onClick={() => void choose("enabled")}
            >
              {t("usage_accept")}
            </button>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
