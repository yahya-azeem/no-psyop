import { openUrl } from '@tauri-apps/plugin-opener';

const inTauri =
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/**
 * Open an external URL in the user's default browser.
 *
 * Under Tauri this goes through the native opener plugin (launches the OS
 * default browser). In a plain browser/PWA build it opens a new tab.
 */
export function openExternal(url: string) {
  if (!/^https?:\/\//i.test(url)) return;
  if (inTauri) {
    // Best-effort: if the plugin isn't registered, fall back to a new tab.
    openUrl(url).catch(() => {
      const w = window.open(url, '_blank', 'noopener,noreferrer');
      if (w) w.opener = null;
    });
  } else {
    const w = window.open(url, '_blank', 'noopener,noreferrer');
    if (w) w.opener = null;
  }
}