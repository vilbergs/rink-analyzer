import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

const BACKGROUND = "#0f1115";

export default defineConfig({
  plugins: [
    svelte(),
    VitePWA({
      registerType: "autoUpdate",
      includeAssets: ["apple-touch-icon.png"],
      manifest: {
        name: "Rink Replay",
        short_name: "Rink Replay",
        description: "Hockey positioning, one play at a time.",
        display: "standalone",
        orientation: "portrait",
        theme_color: BACKGROUND,
        background_color: BACKGROUND,
        icons: [
          { src: "icon-192.png", sizes: "192x192", type: "image/png" },
          { src: "icon-512.png", sizes: "512x512", type: "image/png" },
        ],
      },
      workbox: {
        globPatterns: ["**/*.{js,css,html,wasm,png}"],
        globIgnores: ["curriculum/**", "plays/**"],
        maximumFileSizeToCacheInBytes: 4 * 1024 * 1024,
        runtimeCaching: [
          {
            urlPattern: ({ url }) => /^\/(curriculum|plays)\//.test(url.pathname),
            handler: "StaleWhileRevalidate",
            options: { cacheName: "content" },
          },
        ],
      },
    }),
  ],
});
