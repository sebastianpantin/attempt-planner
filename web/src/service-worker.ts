/** Where the page registers its service worker, next to `index.html`. */
export const SERVICE_WORKER = "sw.js";

/**
 * How long the page waits for the network before showing the cached copy, so a weak
 * signal at a meet venue does not leave the lifter waiting.
 */
const PAGE_TIMEOUT_MS = 3000;

/** A short hex hash of `text` (FNV-1a), to name a cache after what it holds. */
function hash(text: string): string {
  let value = 0x811c9dc5;
  for (let index = 0; index < text.length; index++) {
    value = Math.imul(value ^ text.charCodeAt(index), 0x01000193);
  }
  return (value >>> 0).toString(16).padStart(8, "0");
}

/** The page's own address and every built file but the service worker, as it fetches them. */
export function cachedUrls(builtFiles: string[]): string[] {
  const files = builtFiles
    .map((file) => file.split("\\").join("/"))
    .filter((file) => file !== SERVICE_WORKER && file !== "index.html");
  return ["./", ...files.map((file) => `./${file}`)].sort();
}

/**
 * The service worker for a build of `builtFiles` with `page` as its `index.html`, so the
 * page works offline once loaded, mid-meet included. It keeps every file in a cache named
 * after the build, so a new build gets a new cache and the old one goes once no open page
 * uses it. The page itself comes from the network when it answers successfully within
 * `PAGE_TIMEOUT_MS`, so an online visit gets the latest build; the other files have their
 * content in their names and never change, so they come from the cache first. Matches
 * ignore `Vary`: a server that varies by `Origin` would otherwise miss the cached copy of
 * a `crossorigin` script.
 */
export function serviceWorkerSource(
  builtFiles: string[],
  page: string,
): string {
  const urls = cachedUrls(builtFiles);
  const cache = `attempt-planner-${hash([...urls, page].join("\n"))}`;
  return `const CACHE = ${JSON.stringify(cache)};
const PAGE_TIMEOUT_MS = ${PAGE_TIMEOUT_MS};
const FILES = ${JSON.stringify(urls)};

self.addEventListener("install", (event) => {
  event.waitUntil(caches.open(CACHE).then((cache) => cache.addAll(FILES)));
});

self.addEventListener("activate", (event) => {
  const old = (key) => key.startsWith("attempt-planner-") && key !== CACHE;
  event.waitUntil(
    caches
      .keys()
      .then((keys) => Promise.all(keys.filter(old).map((key) => caches.delete(key))))
      .then(() => self.clients.claim()),
  );
});

self.addEventListener("fetch", (event) => {
  const { request } = event;
  if (request.method !== "GET") return;
  const cached = (key) =>
    caches.open(CACHE).then((cache) => cache.match(key, { ignoreVary: true }));
  if (request.mode === "navigate") {
    const fromNetwork = fetch(request).then((response) =>
      response.ok ? response : cached("./").then((hit) => hit || response),
    );
    const slow = new Promise((resolve) => setTimeout(resolve, PAGE_TIMEOUT_MS))
      .then(() => cached("./"))
      .then((hit) => hit || fromNetwork);
    event.respondWith(
      Promise.race([fromNetwork, slow]).catch(() => cached("./")),
    );
    return;
  }
  event.respondWith(cached(request).then((hit) => hit || fetch(request)));
});
`;
}
