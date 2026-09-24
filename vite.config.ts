import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // Tailwind v4 用到 oklch() / color-mix()，需要 Chrome 111+。
  // 本机 WebView2 实测 152.0.4191.66（Evergreen 自动更新），远高于门槛。
  build: {
    target: "chrome114",
    sourcemap: false,
    // 两个入口（2026-09-24）：面板本身，与真实浏览器采集页 `probe.html` ——
    // 后者由 `qb-app::browser_probe` 在 127.0.0.1 上临时供给系统默认浏览器，
    // 跟面板共用同一份 `signals.ts`。
    rolldownOptions: {
      input: {
        main: fileURLToPath(new URL("./index.html", import.meta.url)),
        probe: fileURLToPath(new URL("./probe.html", import.meta.url)),
      },
    },
  },
});
