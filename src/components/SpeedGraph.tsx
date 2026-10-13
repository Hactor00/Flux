import { memo, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
export const SpeedGraph = memo(function SpeedGraph({
  speed,
}: {
  speed: number;
}) {
  const { t } = useTranslation();
  const latest = useRef(speed);
  latest.current = speed;
  const [samples, setSamples] = useState<number[]>(Array(60).fill(0));
  useEffect(() => {
    const timer = setInterval(
      () => setSamples((values) => [...values.slice(1), latest.current]),
      250,
    );
    return () => clearInterval(timer);
  }, []);
  const maximum = Math.max(1, ...samples);
  const points = samples
    .map((value, index) => `${index * 10},${64 - (value / maximum) * 56}`)
    .join(" ");
  return (
    <svg
      className="speed-graph"
      viewBox="0 0 590 72"
      role="img"
      aria-label={t("live_network")}
      preserveAspectRatio="none"
    >
      <defs>
        <linearGradient id="flux-speed" x1="0" x2="0" y1="0" y2="1">
          <stop offset="0" stopColor="#b39af4" stopOpacity=".25" />
          <stop offset="1" stopColor="#b39af4" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d="M0 64H590" stroke="#ffffff0c" />
      <polygon points={`0,72 ${points} 590,72`} fill="url(#flux-speed)" />
      <polyline
        points={points}
        fill="none"
        stroke="#b39af4"
        strokeWidth="1.6"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
});
