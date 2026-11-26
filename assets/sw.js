const CACHE = 'taskapp-static-v1';
const SHELL = '/sin-conexion';
const STATIC = [SHELL, '/app.css', '/app.js', '/offline.js', '/manifest.webmanifest',
  '/icon-180.png', '/icon-192.png', '/icon-512.png', '/icon-maskable.png'];

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
  const navigation = request.mode === 'navigate' && (
    ['/', '/historial', '/login', '/exportar', SHELL].includes(url.pathname)
    || /^\/tareas\/(nueva|\d+)$/.test(url.pathname)
  );
  if (navigation || STATIC.includes(url.pathname)) {
    event.respondWith(fetch(request).catch(async () => (
      await (await caches.open(CACHE)).match(navigation ? SHELL : url.pathname) || Response.error()
    )));
  }
});
