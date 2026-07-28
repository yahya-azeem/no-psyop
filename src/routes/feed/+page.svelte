<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { feed, isFetching, isCaughtUp, visiblePosts, addPosts, clearFeed } from '$lib/stores/feed';
  import type { FeedItem } from '$lib/types';

  let items = $derived($visiblePosts);

  async function fetchFeed() {
    isFetching.set(true);
    try {
      const result = await invoke<FeedItem[]>('get_feed', {});
      if (result.length === 0) {
        isCaughtUp.set(true);
      } else {
        addPosts(result);
      }
    } catch (e) {
      console.error('fetch feed failed', e);
    } finally {
      isFetching.set(false);
    }
  }

  function formatTimestamp(ts: number) {
    const d = new Date(ts * 1000);
    const now = Date.now();
    const diff = now - d.getTime();
    const hours = Math.floor(diff / 3600000);
    if (hours < 1) return 'just now';
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }
</script>

<div class="feed-page">
  <div class="feed-header">
    <h2 class="feed-title">Feed</h2>
    <p class="feed-subtitle">curated posts from your networks</p>
  </div>

  <div class="feed-meta">
    <span class="feed-count">{items.length} items</span>
  </div>

  <div class="feed-actions">
    <button class="btn btn-primary" onclick={fetchFeed} disabled={$isFetching}>
      {$isFetching ? 'Fetching...' : 'Refresh feed'}
    </button>
    <button class="btn btn-ghost" onclick={clearFeed}>Clear</button>
  </div>

  <div class="feed-list">
    {#if items.length === 0 && !$isFetching}
      <div class="feed-empty">
        <p>No posts yet.</p>
        <p class="feed-empty-hint">Connect your accounts in Settings, then fetch your feed.</p>
      </div>
    {/if}

    {#each items as item (item.post.id)}
      <article class="post-card">
        <div class="post-header">
          <span class="post-platform">{item.post.platform}</span>
          <span class="post-author">{item.post.author_username}</span>
          <span class="post-time">{formatTimestamp(item.post.timestamp)}</span>
        </div>

        <div class="post-body">
          <p class="post-content">{item.post.content}</p>
          {#if item.post.media_urls.length > 0}
            <div class="post-media">
              {#each item.post.media_urls as url}
                <img src={url} alt="media" loading="lazy" />
              {/each}
            </div>
          {/if}
        </div>

        <div class="post-footer">
          <span class="post-score" title="Relevance score">
            {(item.relevance_score * 100).toFixed(0)}% match
          </span>
        </div>
      </article>
    {/each}

    {#if $isCaughtUp}
      <div class="feed-end">
        <div class="feed-end-line"></div>
        <p class="feed-end-text">You are all caught up.</p>
        <div class="feed-end-line"></div>
      </div>
    {/if}
  </div>
</div>

<style>
  .feed-page {
    max-width: var(--max-width);
  }

  .feed-header {
    margin-bottom: 1.5rem;
  }

  .feed-title {
    font-size: 1.5rem;
    font-weight: 600;
    margin: 0;
    letter-spacing: -0.02em;
  }

  .feed-subtitle {
    font-size: 0.85rem;
    color: var(--fg-muted);
    margin: 0.25rem 0 0 0;
  }

  .feed-meta {
    margin-bottom: 1rem;
  }

  .feed-count {
    font-size: 0.8rem;
    color: var(--fg-muted);
  }

  .feed-actions {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
  }

  .btn {
    padding: 0.5rem 1rem;
    border-radius: var(--radius);
    border: 1px solid var(--border);
    font-size: 0.85rem;
    transition: all 0.15s;
  }

  .btn-primary {
    background: var(--accent);
    color: white;
    border-color: var(--accent);
  }

  .btn-primary:hover {
    background: var(--accent-hover);
  }

  .btn-primary:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn-ghost {
    background: transparent;
    color: var(--fg-muted);
  }

  .btn-ghost:hover {
    background: var(--bg);
    color: var(--fg);
  }

  .feed-list {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .feed-empty {
    text-align: center;
    padding: 3rem 0;
    color: var(--fg-muted);
  }

  .feed-empty-hint {
    font-size: 0.85rem;
    margin-top: 0.5rem;
  }

  .post-card {
    background: var(--bg-card);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 1rem;
    filter: grayscale(100%);
    transition: filter 0.2s;
  }

  .post-card:hover {
    filter: grayscale(0%);
  }

  .post-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
    font-size: 0.8rem;
  }

  .post-platform {
    font-weight: 600;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
  }

  .post-author {
    color: var(--fg);
    font-weight: 500;
  }

  .post-time {
    margin-left: auto;
    color: var(--fg-muted);
  }

  .post-body {
    margin-bottom: 0.75rem;
  }

  .post-content {
    margin: 0;
    line-height: 1.6;
    font-size: 0.95rem;
  }

  .post-media {
    margin-top: 0.75rem;
    border-radius: var(--radius);
    overflow: hidden;
  }

  .post-media img {
    width: 100%;
    border-radius: var(--radius);
  }

  .post-footer {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    font-size: 0.78rem;
  }

  .post-score {
    color: var(--fg-muted);
  }

  .feed-end {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 2rem 0;
  }

  .feed-end-line {
    flex: 1;
    height: 1px;
    background: var(--border);
  }

  .feed-end-text {
    margin: 0;
    font-size: 0.85rem;
    color: var(--fg-muted);
    white-space: nowrap;
    font-style: italic;
  }
</style>
