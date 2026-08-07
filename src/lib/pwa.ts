/**
 * Register the service worker so the app installs and works as a PWA.
 * Only runs over real http(s) hosts — the Tauri webview uses a custom
 * protocol that would reject the SW, so we skip it there.
 */
export function registerPwa() {
  if (typeof window === 'undefined') return;
  if (
    !window.location.protocol.startsWith('http') ||
    !('serviceWorker' in navigator)
  ) {
    return;
  }
  window.addEventListener('load', () => {
    navigator.serviceWorker.register('/sw.js').catch(() => {
      /* SW install is best-effort; ignore failures. */
    });
  });
}