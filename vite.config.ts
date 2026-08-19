import { defineConfig } from "vite";

// 前端源码在 ui/，产物出到仓库根的 dist/（tauri.conf.json 的 frontendDist 指向它）
export default defineConfig({
  root: "ui",
  build: {
    outDir: "../dist",
    emptyOutDir: true,
    // WebKitGTK / WKWebView / WebView2 都是较新的引擎，不用为老浏览器降级
    target: "es2022",
  },
  server: {
    port: 5173,
    strictPort: true,
  },
  // Tauri 的错误信息里带源码位置才好排查
  clearScreen: false,
});
