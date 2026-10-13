import type { Progress, Task } from "./api";

export function transferView(task: Task, progress?: Progress) {
  const live =
    task.status === "running" && progress?.id === task.id
      ? progress
      : undefined;
  const done = live?.completed ?? task.completed;
  const total = live ? live.total : task.total;
  const estimated = live?.total_estimated ?? task.total_estimated ?? false;
  const media = live?.media ?? task.media_progress;
  const rate = live?.bytes_per_second || 0;
  const phase =
    task.status === "running"
      ? media?.phase
      : task.status === "verifying" && task.request.media
        ? "processing"
        : undefined;
  const percent =
    task.status === "complete"
      ? 100
      : (media?.percent ??
        (total != null && total > 0
          ? Math.min(99.9, (100 * done) / total)
          : null));
  const eta =
    task.status !== "running" || phase === "processing" || phase === "live"
      ? null
      : (media?.eta_seconds ??
        (total && rate
          ? Math.max(1, Math.ceil(Math.max(0, total - done) / rate))
          : null));
  return {
    done,
    total,
    estimated,
    media,
    rate,
    phase,
    percent: phase === "live" ? null : percent,
    eta,
    percentEstimated:
      estimated || (!!media && total == null && media.percent != null),
  };
}
export function formatDuration(seconds: number) {
  const value = Math.max(0, Math.floor(seconds));
  return `${Math.floor(value / 3600)
    .toString()
    .padStart(2, "0")}:${Math.floor((value % 3600) / 60)
    .toString()
    .padStart(2, "0")}:${(value % 60).toString().padStart(2, "0")}`;
}
