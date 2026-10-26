import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles.css";
import App from "./App";
import Surface from "./Surface";
import "./design.css";
import "./modern.css";
import { Updater } from "./components/Updater";
import { TooltipProvider } from "@radix-ui/react-tooltip";
import { applyTheme, storedTheme } from "./theme";
import { UsageStatistics } from "./components/UsageStatistics";
applyTheme(storedTheme());
matchMedia("(prefers-color-scheme: light)").addEventListener("change", () =>
  applyTheme(storedTheme()),
);
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <TooltipProvider delayDuration={350}>
      {new URLSearchParams(location.search).has("view") ? <Surface /> : <App />}
      {!new URLSearchParams(location.search).has("view") && <Updater startup />}
      {!new URLSearchParams(location.search).has("view") && (
        <UsageStatistics startup />
      )}
    </TooltipProvider>
  </React.StrictMode>,
);
