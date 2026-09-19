// QB Gate — Copyright (C) 2026 smithtaylor7748-ops
// Licensed under AGPL-3.0-only with the additional terms permitted by its section 7:
// see LICENSE and LICENSE-ADDITIONAL-TERMS.md at the repository root.
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { QueryClientProvider } from "@tanstack/react-query";
import { queryClient } from "./lib/store";
import { HashRouter } from "react-router-dom";
import "./styles/index.css";

// 主题在第一帧之前定下来。Shell 里写 data-theme 的 effect 要等首次渲染提交之后才跑，
// 手选的主题跟系统不一致时，窗口会先按系统色画一帧再跳过去。
document.documentElement.dataset.theme =
  localStorage.getItem("qb-theme") ?? "system";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <QueryClientProvider client={queryClient}>
      <HashRouter>
        <App />
      </HashRouter>
    </QueryClientProvider>
  </React.StrictMode>,
);
