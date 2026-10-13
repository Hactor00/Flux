import { useTranslation } from "react-i18next";
import type { SeedingRule } from "../api";
import { Choice } from "./primitives";

export function SeedingRules({
  value,
  onChange,
  inherit = false,
}: {
  value: SeedingRule | null;
  onChange: (value: SeedingRule | null) => void;
  inherit?: boolean;
}) {
  const { t } = useTranslation();
  const ratio = value && "ratio_milli" in value ? value.ratio_milli : 1000;
  const minutes = value && "minutes" in value ? value.minutes : 60;
  function mode(mode: string) {
    if (mode === "inherit") onChange(null);
    else if (mode === "ratio") onChange({ mode, ratio_milli: ratio });
    else if (mode === "duration") onChange({ mode, minutes });
    else if (mode === "either") onChange({ mode, ratio_milli: ratio, minutes });
    else onChange({ mode: mode as "disabled" | "infinite" });
  }
  return (
    <div className="seeding-rules">
      <label>
        {t("seeding_rule")}
        <Choice
          label={t("seeding_rule")}
          value={value?.mode ?? "inherit"}
          onChange={mode}
          options={[
            ...(inherit
              ? [{ value: "inherit", label: t("global_default") }]
              : []),
            { value: "ratio", label: t("seed_ratio") },
            { value: "duration", label: t("seed_duration") },
            { value: "either", label: t("seed_either") },
            { value: "infinite", label: t("seed_infinite") },
            { value: "disabled", label: t("seed_disabled") },
          ]}
        />
      </label>
      {value && "ratio_milli" in value && (
        <label>
          {t("ratio")}
          <input
            type="number"
            min={0.001}
            max={500}
            step={0.001}
            value={ratio / 1000}
            onChange={(event) =>
              onChange({
                ...value,
                ratio_milli: Math.round(Number(event.target.value) * 1000),
              })
            }
          />
        </label>
      )}
      {value && "minutes" in value && (
        <label>
          {t("seed_minutes")}
          <input
            type="number"
            min={1}
            max={525600}
            step={1}
            value={minutes}
            onChange={(event) =>
              onChange({
                ...value,
                minutes: Math.round(Number(event.target.value)),
              })
            }
          />
        </label>
      )}
    </div>
  );
}
