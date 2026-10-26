import { useEffect, type RefObject } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { api } from "./api";

export function useSurfaceSize(
  ref: RefObject<HTMLElement | null>,
  enabled: boolean,
) {
  useEffect(() => {
    if (!enabled || !isTauri() || !location.search.includes("view=")) return;
    let timer: ReturnType<typeof setTimeout>;
    let frame = 0;
    let disposed = false;
    let previous = 0;
    let observer: ResizeObserver | undefined;
    const attach = () => {
      if (disposed) return;
      // Radix mounts portal contents after the parent's first effect.
      const element = ref.current;
      if (!element) {
        frame = requestAnimationFrame(attach);
        return;
      }
      const measure = () => {
        clearTimeout(timer);
        timer = setTimeout(() => {
          if (disposed) return;
          const height = Math.ceil(
            Math.max(
              element.scrollHeight,
              element.getBoundingClientRect().height,
            ),
          );
          if (Math.abs(previous - height) <= 1) return;
          previous = height;
          void api.resizeSurface(height).catch(() => {});
        }, 50);
      };
      observer = new ResizeObserver(measure);
      observer.observe(element);
      measure();
    };
    attach();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      observer?.disconnect();
      clearTimeout(timer);
    };
  }, [ref, enabled]);
}
