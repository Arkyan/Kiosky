import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Configuration recommandée par Tauri : port fixe, pas de rechargement sur src-tauri.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ["**/src-tauri/**"] },
  },
  build: {
    target: "chrome110", // WebView2 est basé sur Chromium
    minify: "esbuild",
    sourcemap: false,
  },
});
