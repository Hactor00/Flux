import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  desktop,
  type CollectionEntry,
  type ResolvedCollection,
} from "../api";
import { formatDuration } from "../transfer";
import { SelectionList } from "./SelectionList";

export function CollectionPicker({
  onChange,
  onBusy,
}: {
  onChange: (entries: CollectionEntry[]) => void;
  onBusy: (busy: boolean) => void;
}) {
  const { t } = useTranslation();
  const [url, setUrl] = useState("");
  const [collection, setCollection] = useState<ResolvedCollection | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const generation = useRef(0);
  useEffect(
    () => () => {
      generation.current++;
      onBusy(false);
      onChange([]);
    },
    [onBusy, onChange],
  );
  useEffect(() => {
    onChange(
      collection?.entries.filter((entry) => selected.has(entry.url)) || [],
    );
  }, [collection, selected, onChange]);
  async function inspect(append = false) {
    const current = ++generation.current;
    setBusy(true);
    onBusy(true);
    setError("");
    try {
      const page = await api.inspectCollection(
        url.trim(),
        append ? collection?.next_offset : 0,
      );
      if (current !== generation.current) return;
      const previous = append ? collection?.entries || [] : [];
      const entries = [
        ...new Map(
          [...previous, ...page.entries].map((entry) => [entry.url, entry]),
        ).values(),
      ].slice(0, 500);
      setCollection({ ...page, entries });
      setSelected(
        (value) =>
          new Set([
            ...(append ? value : []),
            ...page.entries.map((entry) => entry.url),
          ]),
      );
    } catch (error) {
      if (current === generation.current) setError(String(error));
    } finally {
      if (current === generation.current) {
        setBusy(false);
        onBusy(false);
      }
    }
  }
  return (
    <section className="collection-picker">
      <label>
        {t("collection_url")}
        <input
          type="url"
          value={url}
          placeholder="https://"
          onChange={(event) => {
            generation.current++;
            setUrl(event.target.value);
            setCollection(null);
            setSelected(new Set());
            setError("");
            setBusy(false);
            onBusy(false);
          }}
        />
      </label>
      <button
        className="secondary"
        type="button"
        disabled={!desktop || !url.trim() || busy}
        onClick={() => void inspect()}
      >
        {t(busy ? "collection_loading" : "collection_inspect")}
      </button>
      <p className="form-hint">{t("collection_hint")}</p>
      {error && (
        <p className="form-error" role="alert">
          {error}
        </p>
      )}
      {collection && (
        <>
          <div className="collection-heading">
            <strong>{collection.title}</strong>
            <span>{t("collection_selected", { count: selected.size })}</span>
            <button
              type="button"
              className="text-button"
              onClick={() =>
                setSelected(
                  selected.size === collection.entries.length
                    ? new Set()
                    : new Set(collection.entries.map((entry) => entry.url)),
                )
              }
            >
              {t(
                selected.size === collection.entries.length
                  ? "select_none"
                  : "select_all",
              )}
            </button>
          </div>
          <SelectionList
            label={t("collection")}
            items={collection.entries.map((entry) => ({
              key: entry.url,
              title: entry.title,
              detail:
                entry.duration == null ? "—" : formatDuration(entry.duration),
              selected: selected.has(entry.url),
            }))}
            onToggle={(url, checked) =>
              setSelected((value) => {
                const next = new Set(value);
                if (checked) next.add(url);
                else next.delete(url);
                return next;
              })
            }
          />
          {collection.has_more && collection.next_offset < 500 && (
            <button
              className="secondary"
              type="button"
              disabled={busy}
              onClick={() => void inspect(true)}
            >
              {t(busy ? "collection_loading" : "collection_more")}
            </button>
          )}
        </>
      )}
    </section>
  );
}
