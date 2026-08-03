<script lang="ts">
  import { proxiedMedia } from '$lib/media';
  import type { FeedItem } from '$lib/types';

  let { items }: { items: FeedItem[] } = $props();

  const NEWS_SOURCES: Record<string, string> = {
    polymarket: 'Polymarket',
    ajenglish: 'Al Jazeera',
    aljazeera: 'Al Jazeera',
    aljazeeraenglish: 'Al Jazeera',
  };

  let news = $derived(
    items
      .filter((i) => {
        const name = i.post.author_username?.toLowerCase();
        return i.post.platform === 'Twitter' && name ? name in NEWS_SOURCES : false;
      })
      .sort((a, b) => b.post.timestamp - a.post.timestamp)
      .slice(0, 15)
  );

  function sourceLabel(item: FeedItem) {
    return NEWS_SOURCES[item.post.author_username.toLowerCase()] ?? item.post.author_username;
  }

  function formatTimestamp(ts: number) {
    const d = new Date(ts * 1000);
    const diff = Date.now() - d.getTime();
    const hours = Math.floor(diff / 3600000);
    if (hours < 1) return 'just now';
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }
</script>

{#if news.length > 0}
  <section class="news-section">
    <div class="news-header">
      <h3 class="news-title">News</h3>
      <span class="news-sub">live from Polymarket &amp; Al Jazeera</span>
    </div>
    <div class="news-tray">
      {#each news as item (item.post.id)}
        <article class="news-card" aria-label={`${sourceLabel(item)}: ${item.post.content}`}>
          <div class="news-card-top">
            <span class="news-source">{sourceLabel(item)}</span>
            <span class="news-handle">@{item.post.author_username}</span>
            <span class="news-time">{formatTimestamp(item.post.timestamp)}</span>
          </div>
          <p class="news-content">{item.post.content}</p>
          {#if item.post.media_urls[0] && !item.post.is_video}
            <img
              class="news-thumb"
              src={proxiedMedia(item.post.media_urls[0])}
              alt=""
              loading="lazy"
            />
          {/if}
        </article>
      {/each}
    </div>
  </section>
{/if}

<style>
  .news-section { margin-bottom: 1.5rem; }
  .news-header { display: flex; align-items: baseline; gap: 0.75rem; margin-bottom: 0.75rem; }
  .news-title { font-size: 1rem; font-weight: 600; margin: 0; letter-spacing: -0.01em; }
  .news-sub { font-size: 0.75rem; color: var(--fg-muted); }
  .news-tray {
    display: flex;
    gap: 0.75rem;
    overflow-x: auto;
    padding-bottom: 0.5rem;
    scroll-snap-type: x proximity;
  }
  .news-card {
    flex: 0 0 auto;
    width: min(320px, 82vw);
    scroll-snap-align: start;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 0.75rem;
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .news-card-top { display: flex; align-items: center; gap: 0.5rem; font-size: 0.75rem; }
  .news-source {
    font-weight: 700;
    font-size: 0.78rem;
    color: var(--accent);
    letter-spacing: 0.01em;
  }
  .news-handle { color: var(--fg-muted); }
  .news-time { margin-left: auto; color: var(--fg-muted); white-space: nowrap; }
  .news-content {
    margin: 0;
    font-size: 0.88rem;
    line-height: 1.5;
    white-space: pre-wrap;
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 6;
    line-clamp: 6;
  }
  .news-thumb {
    width: 100%;
    max-height: 200px;
    object-fit: cover;
    border-radius: calc(var(--radius) - 2px);
    display: block;
  }
</style>
