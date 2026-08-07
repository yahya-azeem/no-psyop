import type {
  PlatformName,
  ScrapeEnvelope,
  ScrapeItem,
  ScrapeKind,
  ScrapeMessage,
  ScrapeProfile,
} from './types';
import { fetchJson, runScrape } from './session';
import type { ScrapeResult } from './session';

function ts(ms?: string | number): number | undefined {
  const n = Number(ms ?? 0);
  return n ? Math.floor(n / 1000) : undefined;
}

// ---------------------------------------------------------------------------
// Instagram — same-origin /api/v1 (legacy web session) endpoints the desktop
// app already talks to. Session cookie is applied via credentials.
// ---------------------------------------------------------------------------
function instagramItems(body: unknown): ScrapeItem[] {
  const root = (body as any) ?? {};
  const feed = root.feed_items ?? root.data?.user?.edge_web_feed_timeline?.edges ?? [];
  const items: ScrapeItem[] = [];
  for (const raw of feed) {
    const media = raw.media ?? raw.node ?? raw;
    const id = String(media.id ?? media.pk ?? raw.id ?? '');
    if (!id || id === 'null') continue;
    const caption = media.caption_text ?? (media.caption && media.caption.text) ?? '';
    const m: string[] = [];
    if (media.web_media?.media_items) {
      for (const mi of Object.values(media.web_media.media_items)) {
        const url = (mi as any)?.url;
        if (url) m.push(url);
      }
    }
    if (media.candidates) for (const c of media.candidates) if (c?.url) m.push(c.url);
    const user = media.user ?? raw.user ?? media.owner;
    items.push({
      id,
      text: caption,
      author: user?.username ?? media.username,
      authorId: user?.pk ? String(user.pk) : String(user?.id ?? ''),
      media: m,
      isVideo: media.media_type === 2,
      ts: ts(media.taken_at ?? media.timestamp),
    });
  }
  return items;
}

function instagramMessages(body: unknown, kind: ScrapeKind): ScrapeMessage[] {
  if (kind !== 'inbox') return [];
  const raw = (body as any)?.inbox?.threads ?? [];
  const out: ScrapeMessage[] = [];
  for (const t of raw) {
    const convId = String(t.thread_id ?? t.id ?? '');
    for (const node of t.items ?? t.latest_items ?? []) {
      const text = node.text ?? node.message ?? '';
      out.push({
        id: String(node.item_id ?? node.client_context ?? ''),
        conversationId: convId,
        text,
        from: node.user?.username ?? String(node.user_id ?? ''),
        isMine: node.user_id && t.viewer_id ? Number(node.user_id) === Number(t.viewer_id) : false,
        ts: ts(node.timestamp ?? node.replied_at),
      });
    }
  }
  return out;
}

function instagramProfile(body: unknown): ScrapeProfile | undefined {
  const u = (body as any)?.user ?? (body as any)?.user_info;
  if (!u) return undefined;
  return {
    id: String(u.pk ?? u.id ?? ''),
    username: u.username ?? u.full_name ?? '',
    displayName: u.full_name,
    avatar: u.profile_pic_url ?? u.profile_pic_url_hd,
    followers: u.follower_count ?? u.followers?.count,
  };
}

// ---------------------------------------------------------------------------
// Best-effort X scrapers. X's GraphQL op ids are unstable, so we use the
// legacy same-origin JSON when available and otherwise parse embedded HTML.
// ---------------------------------------------------------------------------
function xItems(body: unknown): ScrapeItem[] {
  const root = (body as any) ?? {};
  const global = root.globalObjects?.tweets ?? {};
  return Object.values(global).map((t: any) => ({
    id: String(t.id_str ?? t.id ?? ''),
    text: t.full_text ?? t.text ?? '',
    author: t.user?.screen_name,
    authorId: t.user?.id_str ? String(t.user.id_str) : undefined,
    media: (t.entities?.media ?? []).map((m: any) => m.media_url_https ?? m.media_url).filter(Boolean),
    ts: ts((t.timestamp_ms as any) as string),
  }));
}

// ---------------------------------------------------------------------------
// LinkedIn: only DOM scraping is reliable in-app; scroll + parse feed cards.
// ---------------------------------------------------------------------------
function linkedinItems(doc: Document): ScrapeItem[] {
  const out: ScrapeItem[] = [];
  const cards = doc.querySelectorAll('div.feed-shared-update-v2, div[data-urn]');
  cards.forEach((card) => {
    const urn = card.getAttribute('data-urn') || card.getAttribute('data-activity') || '';
    const textEl = card.querySelector('.update-components-text, .t-post__body-text, [data-test-id="main-feed-activity-card__commentary"]');
    const text = (textEl?.textContent || '').trim();
    const authorEl = card.querySelector('a[href*="/in/"], [data-test-id="main-feed-activity-card__actor-link"]');
    const author = (authorEl?.getAttribute('href') || '').split('/in/')[1]?.split('/')[0] || '';
    const imgs = card.querySelectorAll('img');
    const media: string[] = [];
    for (const img of imgs) {
      const src = img.getAttribute('src');
      if (src && src.startsWith('http')) media.push(src);
    }
    if (text || media.length) {
      out.push({ id: urn || text.slice(0, 24), text, author, media });
    }
  });
  return out;
}

const PLATFORMS: Record<PlatformName, (kind: ScrapeKind) => Promise<ScrapeEnvelope>> = {
  async Instagram(kind) {
    if (kind === 'feed') {
      const body = await fetchJson('/api/v1/feed/timeline/?timezone_offset=0&client_type=ni8mgt4kh4hmg82b', {});
      return { platform: 'Instagram', kind, capturedAt: 0, items: instagramItems(body) };
    }
    if (kind === 'inbox') {
      const body = await fetchJson('/api/v1/direct_v2/inbox/');
      return { platform: 'Instagram', kind, capturedAt: 0, messages: instagramMessages(body, 'inbox') };
    }
    if (kind === 'profile') {
      // profile needs a target; fetch own profile info via the session user id from cookies
      const uid = /sessionid=(\d+)/.exec(document.cookie)?.[1] ?? '';
      const body = await fetchJson(`/api/v1/users/${uid}/info/`);
      const profile = instagramProfile(body);
      return { platform: 'Instagram', kind, capturedAt: 0, profile };
    }
    if (kind === 'stories') {
      const body = await fetchJson('/api/v1/feed/reels_tray/');
      const list = ((body as any)?.tray ?? []).map((u: any) => u?.user?.username).filter(Boolean);
      return { platform: 'Instagram', kind, capturedAt: 0, items: list.map((username: string) => ({ id: username, text: '', author: username, media: [] })) };
    }
    return { platform: 'Instagram', kind, capturedAt: 0 };
  },

  async Twitter(kind) {
    if (kind === 'feed') {
      const body = await fetchJson('/i/api/2/home_timeline.json?count=20');
      return { platform: 'Twitter', kind, capturedAt: 0, items: xItems(body) };
    }
    if (kind === 'news') {
      const body = await fetchJson('/i/api/2/search/adaptive.json?q=POLYMARKET%26f=live%26count=20');
      return { platform: 'Twitter', kind, capturedAt: 0, items: xItems(body) };
    }
    if (kind === 'profile') {
      const body = await fetchJson('/i/users/current.json');
      const me = (body as any) ?? {};
      return { platform: 'Twitter', kind, capturedAt: 0, profile: { id: String(me.id_str ?? ''), username: me.screen_name ?? '', displayName: me.name } };
    }
    return { platform: 'Twitter', kind, capturedAt: 0 };
  },
  async LinkedIn(kind) {
    if (kind === 'feed') {
      // DOM-scrape the (already open, logged-in) feed page.
      const items = linkedinItems(document);
      return { platform: 'LinkedIn', kind, capturedAt: 0, items };
    }
    if (kind === 'profile') {
      const el = document.querySelector('a[href*="/in/"]');
      const username = (el?.getAttribute('href') || '').split('/in/')[1]?.split('/')[0] || '';
      return { platform: 'LinkedIn', kind, capturedAt: 0, profile: { id: username, username } };
    }
    if (kind === 'inbox') {
      const items: ScrapeMessage[] = [];
      document.querySelectorAll('li.msg-conversation-listitem__item, div[data-conversation-id]').forEach((li) => {
        const convId = li.getAttribute('data-conversation-id') || '';
        const label = li.getAttribute('aria-label') || li.textContent || '';
        items.push({ id: convId, conversationId: convId, text: label.trim(), from: '', isMine: false });
      });
      return { platform: 'LinkedIn', kind, capturedAt: 0, messages: items };
    }
    return { platform: 'LinkedIn', kind, capturedAt: 0 };
  },
};

export function scrape(platform: PlatformName, kind: ScrapeKind): Promise<ScrapeResult> {
  return runScrape(platform, kind, PLATFORMS[platform]);
}