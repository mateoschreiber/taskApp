const CACHE = 'taskapp-static-v6';
const SHELL = '/sin-conexion';
const CHARTS = '/graficos/sin-conexion';
// Bump the cache version when replacing the artwork or typeface.
const MEDIA = ['/bricolage-latin.woff2', '/mark.svg', '/jornada.png', '/icon-32.png',
  '/icon-180.png', '/icon-192.png', '/icon-512.png', '/icon-maskable.png'];
const STATIC = [SHELL, CHARTS, '/app.css', '/workspace.css', '/app.js', '/offline.js',
  '/charts.js', '/charts.css', '/manifest.webmanifest', ...MEDIA];

self.addEventListener('install', event => {
  event.waitUntil(caches.open(CACHE).then(cache => cache.addAll(STATIC)));
});

self.addEventListener('activate', event => {
  event.waitUntil((async () => {
    for (const name of await caches.keys()) {
      if (name.startsWith('taskapp-static-') && name !== CACHE) await caches.delete(name);
    }
    await self.clients.claim();
  })());
});

self.addEventListener('fetch', event => {
  const { request } = event;
  const url = new URL(request.url);
  if (url.origin !== self.location.origin || request.method !== 'GET') return;
  if (MEDIA.includes(url.pathname)) {
    event.respondWith(caches.open(CACHE).then(async cache => await cache.match(url.pathname) || fetch(request)));
    return;
  }
  const navigation = request.mode === 'navigate' && (
    ['/', '/historial', '/login', '/exportar', '/graficos', SHELL, CHARTS].includes(url.pathname)
    || /^\/tareas\/(nueva|\d+)$/.test(url.pathname)
  );
  if (navigation || STATIC.includes(url.pathname)) {
    event.respondWith(fetch(request).catch(async () => (
      await (await caches.open(CACHE)).match(navigation
        ? (url.pathname.startsWith('/graficos') ? CHARTS : SHELL)
        : url.pathname) || Response.error()
    )));
  }
});
