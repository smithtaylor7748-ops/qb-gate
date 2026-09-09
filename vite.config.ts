import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  // Tailwind v4 用到 oklch() / color-mix()，需要 Chrome 111+。
  // 本机 WebView2 实测 152.0.4191.66（Evergreen 自动更新），远高于门槛。
  build: { target: "chrome114", sourcemap: false },
});
