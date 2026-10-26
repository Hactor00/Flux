import { useEffect, type RefObject } from "react";
import { isTauri } from "@tauri-apps/api/core";
import { api } from "./api";

export function useSurfaceSize(
  ref: RefObject<HTMLElement | null>,
  enabled: boolean,
) {
  useEffect(() => {
    if (!enabled || !isTauri() || !location.search.includes("view=")) return;
    const element = ref.current;
    if (!element) return;
    let timer: ReturnType<typeof setTimeout>;
    let previous = 0;
    const observer = new ResizeObserver(() => {
      clearTimeout(timer);
      timer = setTimeout(() => {
        const height = Math.ceil(element.getBoundingClientRect().height);
        if (Math.abs(previous - height) <= 1) return;
        previous = height;
        void api.resizeSurface(height).catch(() => {});
      }, 50);
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
      clearTimeout(timer);
    };
  }, [ref, enabled]);
}
