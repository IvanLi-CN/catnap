import fs from "node:fs";
import path from "node:path";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

function readRootCargoVersion(): string | undefined {
  try {
    const rootDir = path.resolve(__dirname, "..");
    const cargoToml = fs.readFileSync(path.join(rootDir, "Cargo.toml"), "utf8");
    const match = cargoToml.match(/^\s*version\s*=\s*"(\d+\.\d+\.\d+)"/m);
    return match?.[1];
  } catch {
    return undefined;
  }
}

const appVersion =
  process.env.VITE_APP_VERSION ??
  process.env.APP_EFFECTIVE_VERSION ??
  readRootCargoVersion() ??
  "0.0.0";

const apiProxyUserHeader = process.env.API_PROXY_USER_HEADER ?? "x-user";
const apiProxyUser = process.env.API_PROXY_USER;

export default defineConfig({
  plugins: [react()],
  define: {
    "import.meta.env.VITE_APP_VERSION": JSON.stringify(appVersion),
  },
  server: {
    port: 18182,
    strictPort: true,
    proxy: {
      "/api": {
        target: process.env.API_PROXY_TARGET ?? "http://localhost:18080",
        changeOrigin: true,
        ...(apiProxyUser
          ? {
              headers: {
                [apiProxyUserHeader]: apiProxyUser,
              },
            }
          : {}),
      },
    },
  },
  preview: {
    port: 18183,
    strictPort: true,
  },
});
