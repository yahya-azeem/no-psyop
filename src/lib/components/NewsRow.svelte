<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { proxiedMedia } from '$lib/media';
  import type { Post } from '$lib/types';

  const NEWS_SOURCES: Record<string, string> = {
    polymarket: 'Polymarket',
    ajenglish: 'Al Jazeera',
    aljazeera: 'Al Jazeera',
    aljazeeraenglish: 'Al Jazeera',
  };

  let news = $state<Post[]>([]);
  let loading = $state(false);
  let error = $state('');

  async function load() {
    loading = true;
    error = '';
    try {
      news = await invoke<Post[]>('get_news');
    } catch (e) {
      error = String(e);
      news = [];
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    load();
  });

  function sourceLabel(post: Post) {
    return NEWS_SOURCES[post.author_username.toLowerCase()] ?? post.author_username;
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

{#if loading && news.length === 0}
  <section class="news-section">
    <div class="news-header">
      <h3 class="news-title">News</h3>
      <span class="news-sub">loading…</span>
    </div>
  </section>
{:else if error && news.length === 0}
  <section class="news-section">
    <div class="news-header">
      <h3 class="news-title">News</h3>
      <span class="news-sub error">unavailable — connect Twitter / start the proxy</span>
    </div>
  </section>
{:else if news.length > 0}
  <section class="news-section">
    <div class="news-header">
      <h3 class="news-title">News</h3>
      <span class="news-sub">live from Polymarket &amp; Al Jazeera</span>
      <button class="news-refresh" onclick={() => load()}>refresh</button>
    </div>
    <div class="news-tray">
      {#each news as post (post.id)}
        <article class="news-card {post.media_urls[0] && !post.is_video ? 'has-media' : 'text-only'}" aria-label={`${sourceLabel(post)}: ${post.content}`}>
          <div class="news-card-top">
            <span class="news-source">{sourceLabel(post)}</span>
            <span class="news-handle">@{post.author_username}</span>
            <span class="news-time">{formatTimestamp(post.timestamp)}</span>
          </div>
          <p class="news-content">{post.content}</p>
          {#if post.media_urls[0] && !post.is_video}
            <img
              class="news-thumb"
              src={proxiedMedia(post.media_urls[0])}
              alt=""
              loading="lazy"
            />
          {/if}
        </article>
      {/each}
    </div>
  </section>
{:else}
  <section class="news-section">
    <div class="news-header">
      <h3 class="news-title">News</h3>
      <span class="news-sub">nothing here yet — refresh to pull latest</span>
      <button class="news-refresh" onclick={() => load()}>refresh</button>
    </div>
  </section>
{/if}

<style>
  .news-section { margin-bottom: 1.5rem; }
  .news-header { display: flex; align-items: baseline; gap: 0.75rem; margin-bottom: 0.75rem; }
  .news-title { font-size: 1rem; font-weight: 600; margin: 0; letter-spacing: -0.01em; }
  .news-sub { font-size: 0.75rem; color: var(--fg-muted); }
  .news-sub.error { color: var(--accent); }
  .news-refresh {
    margin-left: auto;
    font-size: 0.72rem;
    padding: 0.15rem 0.5rem;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg-card);
    color: var(--fg-muted);
    cursor: pointer;
  }
  .news-refresh:hover { border-color: var(--accent); color: var(--accent); }
  .news-tray {
    display: flex;
    align-items: flex-start;
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
  .news-card.text-only .news-content {
    -webkit-line-clamp: 4;
    line-clamp: 4;
  }
  .news-thumb {
    width: 100%;
    max-height: 200px;
    object-fit: cover;
    border-radius: calc(var(--radius) - 2px);
    display: block;
  }
</style>
