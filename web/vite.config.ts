import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vite";
import { VitePWA } from "vite-plugin-pwa";

const BACKGROUND = "#0f1115";

export default defineConfig({
  server: {
    // The footage service (rink-footage-web) answers under /footage; run it with RINK_FOOTAGE_ORIGIN set to this dev server.
    proxy: { "^/footage/(?:api|room|worker)/": process.env.RINK_FOOTAGE_DEV ?? "http://127.0.0.1:8794" },
  },
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
        // Every other path is a page of the app and falls back to index.html, online or off.
        navigateFallbackDenylist: [/^\/footage\/(?:api|room|worker)\//, /^\/(?:curriculum|plays)\//],
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
