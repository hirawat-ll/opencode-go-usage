import { defineConfig } from "vite";

// Tauri drives this dev server, see src-tauri/tauri.conf.json.
export default defineConfig({
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
