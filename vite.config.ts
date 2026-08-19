import { defineConfig } from "vite";
import { resolve } from "node:path";

// 前端源码在 ui/，产物出到仓库根的 dist/（tauri.conf.json 的 frontendDist 指向它）
export default defineConfig({
  root: "ui",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    // WebKitGTK / WKWebView / WebView2 都是较新的引擎，不用为老浏览器降级
    target: "es2022",
    rollupOptions: {
      // 两个窗口两个入口：弹窗和设置页各自独立加载，互不拖累
      input: {
        popup: resolve(__dirname, "ui/index.html"),
        settings: resolve(__dirname, "ui/settings.html"),
      },
    },
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  // Tauri 的错误信息里带源码位置才好排查
  clearScreen: false,
});
