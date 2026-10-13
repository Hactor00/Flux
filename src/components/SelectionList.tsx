import { useCallback, useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { Check } from "lucide-react";

export function SelectionList<T extends string | number>({
  label,
  items,
  disabled,
  onToggle,
}: {
  label: string;
  items: {
    key: T;
    title: string;
    detail: string;
    selected: boolean;
    hash?: string | null;
  }[];
  disabled?: boolean;
  onToggle: (key: T, selected: boolean) => void;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const key = useCallback((index: number) => items[index].key, [items]);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => viewport.current,
    estimateSize: () => 38,
    overscan: 6,
    getItemKey: key,
  });
  return (
    <div
      className="collection-entries"
      ref={viewport}
      role="group"
      aria-label={label}
      style={{ height: Math.min(220, Math.max(38, items.length * 38)) }}
    >
      <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
        {virtualizer.getVirtualItems().map((row) => {
          const item = items[row.index];
          return (
            <label
              className="collection-entry"
              key={item.key}
              style={{
                position: "absolute",
                insetInline: 0,
                top: 0,
                height: 38,
                transform: `translateY(${row.start}px)`,
              }}
            >
              <input
                type="checkbox"
                disabled={disabled}
                checked={item.selected}
                onChange={(event) => onToggle(item.key, event.target.checked)}
              />
              <span title={item.title}>{item.title}</span>
              <small>{item.detail}</small>
              {item.hash && (
                <span
                  className="selection-integrity"
                  title={`SHA-256: ${item.hash}`}
                >
                  <Check size={14} aria-label="SHA-256" />
                </span>
              )}
            </label>
          );
        })}
      </div>
    </div>
  );
}
