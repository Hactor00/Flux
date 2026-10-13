import React from "react";
import ReactDOM from "react-dom/client";
import "./i18n";
import "./styles.css";
import App from "./App";
import Surface from "./Surface";
import { TooltipProvider } from "@radix-ui/react-tooltip";
ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <TooltipProvider delayDuration={350}>
      {new URLSearchParams(location.search).has("view") ? <Surface /> : <App />}
    </TooltipProvider>
  </React.StrictMode>,
);
