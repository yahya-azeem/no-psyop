import type { PlatformName, ScrapeEnvelope, ScrapeKind } from './types';

/** The native host (embedded WebView controller) can expose this so captured
 * envelopes flow back into the app without waiting for a script return value. */
declare global {
  interface Window {
    __onemediaHost?: {
      capture: (envelope: ScrapeEnvelope) => void;
    };
  }
}

export interface ScrapeError {
  code: string;
  message: string;
}

export type ScrapeResult = { ok: true; envelope: ScrapeEnvelope } | { ok: false; error: ScrapeError };

const ORIGINS: Record<PlatformName, string> = {
  Instagram: 'instagram.com',
  Twitter: 'x.com',
  LinkedIn: 'linkedin.com',
};

/** True when we're running on the platform's own origin (cookies apply). */
export function onPlatformOrigin(platform: PlatformName): boolean {
  if (typeof location === 'undefined') return false;
  const host = location.hostname.toLowerCase();
  const needle = ORIGINS[platform].toLowerCase();
  return host === needle || host.endsWith('.' + needle);
}

export function captureCookies(): Record<string, string> {
  const out: Record<string, string> = {};
  if (typeof document === 'undefined') return out;
  for (const pair of (document.cookie || '').split(';')) {
    const idx = pair.indexOf('=');
    if (idx > 0) out[pair.slice(0, idx).trim()] = pair.slice(idx + 1).trim();
  }
  return out;
}

/** Same-origin JSON GET with the page's own cookies + CSRF header. */
export async function fetchJson(url: string, init?: RequestInit): Promise<unknown> {
  const headers: Record<string, string> = {
    Accept: 'application/json',
    ...(init?.headers as Record<string, string> | undefined),
  };
  const csrf = captureCookies();
  if (csrf['ct0']) headers['X-CSRF-Token'] = csrf['ct0'];
  if (csrf['csrftoken']) headers['X-CSRFToken'] = csrf['csrftoken'];
  const res = await fetch(url, { ...init, headers, credentials: 'same-origin' });
  if (!res.ok) {
    throw new Error(`HTTP ${res.status} for ${url}`);
  }
  return res.json();
}

/** Run a platform scrape inside its own logged-in origin. */
export async function runScrape(
  platform: PlatformName,
  kind: ScrapeKind,
  runner: (kind: ScrapeKind) => Promise<ScrapeEnvelope>
): Promise<ScrapeResult> {
  try {
    if (!onPlatformOrigin(platform)) {
      return {
        ok: false,
        error: {
          code: 'wrong_origin',
          message: `This scraper must run on ${ORIGINS[platform]}, not ${typeof location !== 'undefined' ? location.hostname : 'unknown'}.`,
        },
      };
    }
    const envelope = await runner(kind);
    if (!envelope.capturedAt) envelope.capturedAt = Date.now();
    if (typeof window !== 'undefined' && window.__onemediaHost) {
      window.__onemediaHost.capture(envelope);
    }
    return { ok: true, envelope };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { ok: false, error: { code: 'scrape_failed', message } };
  }
}

export function scrapeError(code: string, message: string): ScrapeResult {
  return { ok: false, error: { code, message } };
}