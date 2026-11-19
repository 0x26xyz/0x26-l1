import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

const NODE_TARGET = process.env.VITE_AND_NODE_TARGET ?? "http://127.0.0.1:8545";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 4174,
    strictPort: false,
    host: true,
    proxy: {
      "/rpc": {
        target: NODE_TARGET,
        changeOrigin: true,
        rewrite: (path) => path.replace(/^\/rpc/, ""),
      },
    },
  },
});
