// Bump whenever the cached application or its compression assets change.
const CACHE = "squeeze-static-v15";

self.addEventListener("install", (event) => {
  event.waitUntil((async () => {
    const cache = await caches.open(CACHE);
    const response = await fetch("/");
    const html = await response.clone().text();
    const shellAssets = [...html.matchAll(/(?:src|href)="([^"]+)"/g)]
      .map((match) => match[1])
      .filter((path) => path?.startsWith("/"));
    const manifestResponse = await fetch("/precache-manifest.json", { cache: "no-store" });
    if (!manifestResponse.ok) throw new Error("Offline asset manifest is unavailable");
    const requiredAssets = await manifestResponse.json();
    await cache.put("/", response);
    await cache.addAll([...new Set([...shellAssets, ...requiredAssets])]);
  })());
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(Promise.all([
    self.clients.claim(),
    caches.keys().then((keys) => Promise.all(keys.filter((key) => key.startsWith("squeeze-static-") && key !== CACHE).map((key) => caches.delete(key)))),
  ]));
});

// Cache only same-origin application assets. User files are never fetched here.
self.addEventListener("fetch", (event) => {
  if (event.request.method !== "GET" || new URL(event.request.url).origin !== self.location.origin) return;
  event.respondWith(caches.match(event.request).then((cached) => cached || fetch(event.request).then((response) => {
    if (response.ok) caches.open(CACHE).then((cache) => cache.put(event.request, response.clone()));
    return response;
  })));
});
