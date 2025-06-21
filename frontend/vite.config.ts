/// <reference types="vite/client" />

import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

const host = process.env.HOST_OVERRIDE || process.env.TAURI_DEV_HOST;

export default defineConfig(async () => ({
    plugins: [react()],
    clearScreen: false,
    server: {
        port: 1420,
        strictPort: true,
        host: host || false,
        hmr: host
            ? {
                  protocol: "ws",
                  host,
                  port: 1421
              }
            : undefined
    },
    resolve: {
        alias: [{ find: "@", replacement: path.resolve(__dirname, "./src") }]
    }
}));
