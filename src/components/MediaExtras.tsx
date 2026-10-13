import * as Switch from "@radix-ui/react-switch";
import { useTranslation } from "react-i18next";
import type { Dispatch, SetStateAction } from "react";
import type { MediaOptions, SubtitleTrack } from "../api";
import { Choice } from "./primitives";

export interface MediaExtrasValue {
  selected: number[];
  subtitleMode: NonNullable<MediaOptions["subtitle_mode"]>;
  normalize: boolean;
  chapters: boolean;
}
export function MediaExtras({
  tracks,
  chapterCount,
  value,
  onChange,
  audioOnly,
  lossless,
}: {
  tracks: SubtitleTrack[];
  chapterCount: number;
  value: MediaExtrasValue;
  onChange: Dispatch<SetStateAction<MediaExtrasValue>>;
  audioOnly: boolean;
  lossless: boolean;
}) {
  const { t } = useTranslation();
  return (
    <details className="download-advanced media-extras">
      <summary>{t("media_extras")}</summary>
      <label>
        {t("subtitle_output")}
        <Choice
          label={t("subtitle_output")}
          value={
            audioOnly && value.subtitleMode === "embed"
              ? "external"
              : value.subtitleMode
          }
          onChange={(subtitleMode) =>
            onChange((value) => ({
              ...value,
              subtitleMode: subtitleMode as MediaExtrasValue["subtitleMode"],
            }))
          }
          options={[
            { value: "none", label: t("subtitle_none") },
            ...(!audioOnly
              ? [{ value: "embed", label: t("subtitle_embed") }]
              : []),
            { value: "external", label: t("subtitle_external") },
          ]}
        />
      </label>
      {tracks.length ? (
        tracks.map((track, index) => (
          <label
            className="switch-row toggle-setting"
            key={`${track.url || track.index}-${track.language}`}
          >
            <span>
              {track.title || track.language} · {track.language}
            </span>
            <Switch.Root
              className="switch"
              aria-label={`${track.title || track.language} ${t("subtitles")}`}
              checked={value.selected.includes(index)}
              onCheckedChange={(checked) =>
                onChange((value) => ({
                  ...value,
                  selected: checked
                    ? [...value.selected, index]
                    : value.selected.filter((item) => item !== index),
                }))
              }
            >
              <Switch.Thumb className="switch-thumb" />
            </Switch.Root>
          </label>
        ))
      ) : (
        <p className="form-hint">{t("subtitles_discovery_hint")}</p>
      )}
      <label className="switch-row toggle-setting">
        <span>{t("normalize_audio")}</span>
        <Switch.Root
          className="switch"
          aria-label={t("normalize_audio")}
          disabled={lossless}
          checked={!lossless && value.normalize}
          onCheckedChange={(normalize) =>
            onChange((value) => ({ ...value, normalize }))
          }
        >
          <Switch.Thumb className="switch-thumb" />
        </Switch.Root>
      </label>
      <p className="form-hint">
        {lossless ? t("lossless_audio_hint") : t("normalize_audio_hint")}
      </p>
      <label className="switch-row toggle-setting">
        <span>
          {t("include_chapters")} · {chapterCount}
        </span>
        <Switch.Root
          className="switch"
          aria-label={t("include_chapters")}
          checked={value.chapters}
          disabled={!chapterCount}
          onCheckedChange={(chapters) =>
            onChange((value) => ({ ...value, chapters }))
          }
        >
          <Switch.Thumb className="switch-thumb" />
        </Switch.Root>
      </label>
    </details>
  );
}
