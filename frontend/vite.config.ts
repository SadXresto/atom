import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

const apiTarget = process.env.ATOM_API ?? "http://127.0.0.1:8787";
// The cloud preview hostname changes per VM, so it is supplied by the runner
// rather than baked into the repository.
const allowedHosts = [process.env.VITE_PREVIEW_HOST].filter(Boolean) as string[];

export default defineConfig({
  plugins: [react(), tailwindcss()],
  server: {
    host: "0.0.0.0",
    port: 3003,
    strictPort: true,
    allowedHosts: allowedHosts.length ? allowedHosts : undefined,
    proxy: {
      "/api": { target: apiTarget, changeOrigin: false },
    },
  },
  build: {
    outDir: "dist",
    sourcemap: false,
  },
});
