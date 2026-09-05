import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

// https://vitejs.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: "localhost",
  },

  // Environment variables starting with TAURI_ will be exposed
  // in tauri-app's webview, prefixed with VITE_.
  envPrefix: ["VITE_", "TAURI_"],
  build: {
    // Tauri uses Chromium on Windows and WebKit on macOS and Linux
    target: process.env.TAURI_PLATFORM == "windows" ? "chrome105" : "safari13",
    minify: !process.env.TAURI_DEBUG ? "esbuild" : false,
    // Produce sourcemaps for debug builds
    sourcemap: !!process.env.TAURI_DEBUG,
    // 多页面入口：主窗口 + 截图翻译窗口
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        screenshot: resolve(__dirname, "screenshot.html"),
      },
    },
  },
}));