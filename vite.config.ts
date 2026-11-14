import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri expects a fixed dev port and forwards `TAURI_DEV_HOST` for device testing.
const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: {
      ignored: ["**/src-tauri/**", "**/.setup/**"],
    },
  },
  build: {
    // WebView2 Evergreen baseline shipped with Windows 11 22H2.
    target: "chrome110",
    sourcemap: false,
    chunkSizeWarningLimit: 1200,
    outDir: "dist",
    emptyOutDir: true,
  },
});
