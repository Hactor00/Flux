import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";
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
