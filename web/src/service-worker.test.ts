import { expect, it } from "vitest";
import { cachedUrls, serviceWorkerSource } from "./service-worker.ts";

const build = [
  "index.html",
  "sw.js",
  "assets/index-a1.js",
  "assets/worker-b2.js",
  "assets\\attempt_sim_wasm_bg-c3.wasm",
];

it("keeps the page and every built file but itself", () => {
  expect(cachedUrls(build)).toEqual([
    "./",
    "./assets/attempt_sim_wasm_bg-c3.wasm",
    "./assets/index-a1.js",
    "./assets/worker-b2.js",
  ]);
});

it("names its cache after the build, so a new build starts a new cache", () => {
  const cacheName = (files: string[], page = "<html>") =>
    /const CACHE = "(attempt-planner-[0-9a-f]{8})"/.exec(
      serviceWorkerSource(files, page),
    )?.[1];
  expect(cacheName(build)).toBeDefined();
  expect(cacheName([...build].reverse())).toBe(cacheName(build));
  expect(cacheName([...build, "assets/index-d4.css"])).not.toBe(
    cacheName(build),
  );
  expect(cacheName(build, "<html lang=en>")).not.toBe(cacheName(build));
});

it("is a script that parses", () => {
  expect(() => new Function(serviceWorkerSource(build, ""))).not.toThrow();
});
