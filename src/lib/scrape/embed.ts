// Entry point for bundling the scraper engine into a single `web-session.js`
// that a native (iOS/Android) WebView injects on the platform's own origin.
import type { PlatformName, ScrapeEnvelope, ScrapeKind } from './types';
import { captureCookies, onPlatformOrigin } from './session';
import { scrape } from './platforms';

declare global {
  interface Window {
    __onemediaHost?: { capture: (envelope: ScrapeEnvelope) => void };
    __onemedia?: {
      scrape: (platform: PlatformName, kind: ScrapeKind) => Promise<
        | { ok: true; envelope: ScrapeEnvelope }
        | { ok: false; error: { code: string; message: string } }
      >;
      isOk: (platform: PlatformName) => boolean;
      cookies: () => Record<string, string>;
    };
  }
}

// The native host is expected to define `window.__onemediaHost` (see session.ts).
// This exposes the canonical entry points to drive scrapes from evaluateJavaScript.
function install() {
  if (typeof window === 'undefined') return;
  window.__onemedia = {
    scrape: (platform: PlatformName, kind: ScrapeKind) => scrape(platform, kind),
    isOk: (platform: PlatformName) => onPlatformOrigin(platform),
    cookies: () => captureCookies(),
  };
}

install();