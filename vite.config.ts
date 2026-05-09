import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { resolve } from "path";

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    rollupOptions: {
      input: {
        main: resolve(__dirname, "index.html"),
        overlay: resolve(__dirname, "src/overlays/overlay.html"),
        actionPopup: resolve(__dirname, "src/overlays/action-popup.html"),
        screenClip: resolve(__dirname, "src/overlays/screen-clip.html"),
        palette: resolve(__dirname, "src/palette/palette.html"),
      },
    },
  },
});
