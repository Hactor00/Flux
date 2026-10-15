import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles.css";
import App from "./App";
import Surface from "./Surface";
import { Updater } from "./components/Updater";
import { TooltipProvider } from "@radix-ui/react-tooltip";
const savedTheme = localStorage.getItem("flux-theme");
document.documentElement.dataset.theme = savedTheme === "light" ? "light" : "dark";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <TooltipProvider delayDuration={350}>
      {new URLSearchParams(location.search).has("view") ? <Surface /> : <App />}
      {!new URLSearchParams(location.search).has("view") && <Updater startup />}
    </TooltipProvider>
  </React.StrictMode>,
);
