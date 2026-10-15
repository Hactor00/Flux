import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";
import { readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
export default defineConfig(({ mode }) => ({
  cacheDir: `.tools/vite-${mode}`,
  plugins: [react(), tailwind()],
  clearScreen: false,
  server:
    mode === "e2e"
      ? {
          host: "127.0.0.1",
          port: 1422,
          strictPort: true,
          proxy: {
            "/__flux_test": {
              target: "http://127.0.0.1:1421",
              headers: {
                "x-flux-test-token": process.env.FLUX_TEST_TOKEN || "",
              },
            },
          },
        }
      : undefined,
  build: {
    rollupOptions: {
      output: {
        manualChunks: {
          react: ["react", "react-dom", "react-dom/client"],
          desktop: [
            "@tauri-apps/api",
            "@tauri-apps/plugin-updater",
            "@tauri-apps/plugin-process",
          ],
          localization: [
            "i18next",
            "react-i18next",
            ...readdirSync(new URL("./src/locales/", import.meta.url))
              .filter((name) => name.endsWith(".json"))
              .map((name) =>
                fileURLToPath(
                  new URL(`./src/locales/${name}`, import.meta.url),
                ),
              ),
          ],
          primitives: [
            "@radix-ui/react-dialog",
            "@radix-ui/react-tabs",
            "@radix-ui/react-select",
            "@radix-ui/react-alert-dialog",
            "@radix-ui/react-switch",
            "@radix-ui/react-tooltip",
          ],
        },
      },
    },
  },
}));
