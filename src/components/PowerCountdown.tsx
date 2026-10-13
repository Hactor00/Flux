import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useTranslation } from "react-i18next";
import { api, subscribe, type PendingPower } from "../api";

export function PowerCountdown() {
  const { t } = useTranslation();
  const [pending, setPending] = useState<PendingPower | null>(null);
  const [now, setNow] = useState(Date.now());
  const [error, setError] = useState("");
  useEffect(() => {
    let alive = true;
    void api
      .pendingPower()
      .then((value) => {
        if (alive) setPending(value);
      })
      .catch((e) => setError(String(e)));
    const events = subscribe<PendingPower | null>("power-pending", (event) => {
      if (alive) setPending(event.payload);
    });
    const timer = setInterval(() => setNow(Date.now()), 250);
    return () => {
      alive = false;
      clearInterval(timer);
      void events.then((stop) => stop());
    };
  }, []);
  const cancel = () => {
    void api.cancelPower().catch((e) => setError(String(e)));
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open) cancel();
      }}
    >
      <Dialog.Portal>
        <Dialog.Overlay className="modal-overlay" />
        <Dialog.Content
          className="modal power-countdown"
          onEscapeKeyDown={cancel}
          onPointerDownOutside={(event) => event.preventDefault()}
        >
          <Dialog.Title>{t("completion_action")}</Dialog.Title>
          <Dialog.Description>
            {t("power_countdown", {
              action: t(`action_${pending?.action || "none"}`),
              seconds: pending
                ? Math.max(0, Math.ceil((pending.deadline - now) / 1000))
                : 30,
            })}
          </Dialog.Description>
          <p role="alert">{error}</p>
          <button className="primary" onClick={cancel}>
            {t("cancel")}
          </button>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
