import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type HostAccount, type HostProvider } from "../api";
import { Toggle } from "./primitives";

const providers: Record<HostProvider, string> = {
  real_debrid: "Real-Debrid",
  all_debrid: "AllDebrid",
  premiumize: "Premiumize",
};

export function HostAccounts() {
  const { t } = useTranslation();
  const [accounts, setAccounts] = useState<HostAccount[]>([]);
  const [keys, setKeys] = useState<Partial<Record<HostProvider, string>>>({});
  const [busy, setBusy] = useState<HostProvider | null>(null);
  const [notice, setNotice] = useState("");
  useEffect(() => {
    void api
      .hostAccounts()
      .then(setAccounts)
      .catch((error) => setNotice(String(error)));
  }, []);
  async function save(
    account: HostAccount,
    remove = false,
    enabled = account.enabled,
  ) {
    setBusy(account.provider);
    setNotice("");
    try {
      setAccounts(
        await api.saveHostAccount(
          account.provider,
          enabled,
          remove ? "" : keys[account.provider]?.trim() || null,
        ),
      );
      setKeys((previous) => ({ ...previous, [account.provider]: "" }));
      setNotice(t("saved"));
    } catch (error) {
      setNotice(String(error));
    } finally {
      setBusy(null);
    }
  }
  return (
    <div className="host-accounts">
      <p className="muted">{t("host_accounts_help")}</p>
      {(Object.keys(providers) as HostProvider[]).map((provider) => {
        const account = accounts.find(
          (value) => value.provider === provider,
        ) ?? { provider, configured: false, enabled: false };
        return (
          <section className="host-account" key={provider}>
            <h3>{providers[provider]}</h3>
            <Toggle
              label={t("enabled")}
              checked={account.enabled}
              onChange={(value) => {
                if (!busy) void save(account, false, value);
              }}
            />
            <label>
              {t("api_key")}
              <input
                type="password"
                autoComplete="off"
                spellCheck={false}
                maxLength={4096}
                placeholder={t(
                  account.configured ? "key_configured" : "api_key",
                )}
                value={keys[provider] ?? ""}
                onChange={(event) =>
                  setKeys((previous) => ({
                    ...previous,
                    [provider]: event.target.value,
                  }))
                }
              />
            </label>
            <div className="dialog-actions">
              <button
                className="secondary"
                disabled={!!busy || !keys[provider]?.trim()}
                onClick={() => void save(account, false, true)}
              >
                {t("save")}
              </button>
              <button
                className="secondary danger"
                disabled={!!busy || !account.configured}
                onClick={() => void save(account, true)}
              >
                {t("remove")}
              </button>
            </div>
          </section>
        );
      })}
      {notice && <p role="status">{notice}</p>}
    </div>
  );
}
