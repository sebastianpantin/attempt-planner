import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";
import type { Plugin } from "vite";
import { defineConfig } from "vitest/config";
import { SERVICE_WORKER, serviceWorkerSource } from "./src/service-worker.ts";

/** Writes the service worker once the build is out, listing every file it wrote. */
const offline = (): Plugin => ({
  name: "offline",
  apply: "build",
  writeBundle({ dir = "dist" }) {
    const built = readdirSync(dir, { recursive: true, withFileTypes: true })
      .filter((entry) => entry.isFile())
      .map((entry) => relative(dir, join(entry.parentPath, entry.name)));
    const page = readFileSync(join(dir, "index.html"), "utf8");
    writeFileSync(join(dir, SERVICE_WORKER), serviceWorkerSource(built, page));
  },
});

export default defineConfig({
  base: "./",
  worker: { format: "es" },
  build: { target: "es2022" },
  plugins: [offline()],
});
