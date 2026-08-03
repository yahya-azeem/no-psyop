<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import MediaGrid from '$lib/components/MediaGrid.svelte';
  import NewsRow from '$lib/components/NewsRow.svelte';
  import StoriesTray from '$lib/components/StoriesTray.svelte';
  import { feed, isFetching, isCaughtUp, visiblePosts, addPosts, clearFeed } from '$lib/stores/feed';
  import { startupSync } from '$lib/autosync';
  import type { FeedItem, SyncResult } from '$lib/types';

  let items = $derived($visiblePosts);
  let searchQuery = $state('');
  let searchResults = $state<string[]>([]);
  let searching = $state(false);
  let syncing = $state(false);
  let syncMsg = $state('');

  let igQuery = $state('');
  let igResults = $state<FeedItem[]>([]);
  let igSearching = $state(false);

  let prevVersion = $state($startupSync.version);

  $effect(() => {
    const v = $startupSync.version;
    if (v > 0 && v !== prevVersion) {
      fetchFeed();
    }
    prevVersion = v;
  });

  async function doSync() {
    syncing = true;
    syncMsg = '';
    try {
      const result = await invoke<SyncResult>('sync_all');
      syncMsg = `Synced ${result.posts_added} posts.`;
      if (result.errors.length > 0) {
        syncMsg += ` Errors: ${result.errors.join('; ')}`;
      }
    } catch (e) {
      syncMsg = `Sync failed: ${e}`;
    } finally {
      syncing = false;
    }
  }

  async function fetchFeed() {
    isFetching.set(true);
    try {
      const result = await invoke<FeedItem[]>('get_feed', {
        userId: null,
        platform: null,
      });
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

  async function doSearch() {
    if (!searchQuery.trim()) return;
    searching = true;
    try {
      searchResults = await invoke<string[]>('search_posts', {
        query: searchQuery,
        platform: null,
      });
    } catch (e) {
      console.error('search failed', e);
    } finally {
      searching = false;
    }
  }

  async function doIgSearch() {
    if (!igQuery.trim()) return;
    igSearching = true;
    igResults = [];
    try {
      const posts = await invoke<any[]>('search_instagram', { query: igQuery });
      igResults = posts.map((p: any) => ({
        post: p,
        proximity_score: 0,
        relevance_score: 1,
      }));
    } catch (e) {
      console.error('ig search failed', e);
    } finally {
      igSearching = false;
    }
  }
</script>

<div class="feed-page">
  <div class="feed-header">
    <h2 class="feed-title">Feed</h2>
    <p class="feed-subtitle">curated posts from your networks</p>
  </div>

  <StoriesTray />

  {#if syncMsg}
    <div class="sync-bar">{syncMsg}</div>
  {/if}

  <div class="feed-meta">
    <span class="feed-count">{items.length} items</span>
  </div>

  <div class="feed-actions">
    <button class="btn btn-primary" onclick={doSync} disabled={syncing}>
      {syncing ? 'Syncing…' : 'Sync now'}
    </button>
    <button class="btn btn-ghost" onclick={fetchFeed} disabled={$isFetching}>
      {$isFetching ? 'Loading…' : 'Refresh feed'}
    </button>
    <button class="btn btn-ghost" onclick={clearFeed}>Clear</button>
  </div>

  <details class="search-section">
    <summary class="search-summary">Search indexed posts</summary>
    <div class="search-area">
      <div class="search-row">
        <input
          type="text"
          placeholder="Search indexed posts…"
          bind:value={searchQuery}
          onkeydown={(e) => e.key === 'Enter' && doSearch()}
          class="search-input"
        />
        <button class="btn btn-ghost" onclick={doSearch} disabled={searching || !searchQuery.trim()}>
          {searching ? '…' : 'Search'}
        </button>
      </div>
      {#if searchResults.length > 0}
        <div class="search-results">
          <span class="search-label">Found {searchResults.length} results</span>
          <ul>
            {#each searchResults as id}
              <li class="search-hit">{id}</li>
            {/each}
          </ul>
          <button class="btn btn-ghost btn-small" onclick={() => searchResults = []}>Clear</button>
        </div>
      {/if}
    </div>
  </details>

  <details class="search-section">
    <summary class="search-summary">Instagram discovery</summary>
    <div class="search-area">
      <div class="search-row">
        <input
          type="text"
          placeholder="Search Instagram for anything…"
          bind:value={igQuery}
          onkeydown={(e) => e.key === 'Enter' && doIgSearch()}
          class="search-input"
        />
        <button class="btn btn-ghost" onclick={doIgSearch} disabled={igSearching || !igQuery.trim()}>
          {igSearching ? '…' : 'Search IG'}
        </button>
      </div>
      {#if igResults.length > 0}
        <MediaGrid items={igResults} />
      {/if}
    </div>
  </details>

  <div class="feed-grid">
    <NewsRow items={items} />
    {#if items.length === 0 && !$isFetching}
      <div class="feed-empty">
        <p>No posts yet.</p>
        <p class="feed-empty-hint">Connect your accounts in Settings, then click Sync now.</p>
      </div>
    {:else}
      <MediaGrid items={items} sectioned />
    {/if}

    {#if $isCaughtUp}
      <div class="feed-end">
        <div class="feed-end-line"></div>
        <p class="feed-end-text">you are all caught up</p>
        <div class="feed-end-line"></div>
      </div>
    {/if}
  </div>
</div>

<style>
  .feed-page { max-width: var(--max-width); }
  .feed-header { margin-bottom: 1.5rem; }
  .feed-title { font-size: 1.5rem; font-weight: 600; margin: 0; letter-spacing: -0.02em; }
  .feed-subtitle { font-size: 0.85rem; color: var(--fg-muted); margin: 0.25rem 0 0 0; }
  .sync-bar { background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius); padding: 0.5rem 0.75rem; font-size: 0.85rem; margin-bottom: 1rem; }
  .search-section { margin-bottom: 1rem; }
  .search-summary { cursor: pointer; font-size: 0.85rem; color: var(--accent); font-weight: 500; padding: 0.25rem 0; }
  .search-area { padding: 0.5rem 0; }
  .search-row { display: flex; gap: 0.5rem; }
  .search-input { flex: 1; padding: 0.5rem 0.75rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-card); color: var(--fg); font-size: 0.9rem; }
  .search-results { margin-top: 0.5rem; padding: 0.5rem 0.75rem; background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius); font-size: 0.85rem; }
  .search-label { color: var(--fg-muted); font-size: 0.8rem; }
  .search-results ul { margin: 0.25rem 0; padding-left: 1rem; }
  .search-hit { font-family: var(--font-mono); font-size: 0.8rem; color: var(--accent); }
  .feed-meta { margin-bottom: 1rem; }
  .feed-count { font-size: 0.8rem; color: var(--fg-muted); }
  .feed-actions { display: flex; gap: 0.5rem; margin-bottom: 1.5rem; flex-wrap: wrap; }
  .btn { padding: 0.5rem 1rem; border-radius: var(--radius); border: 1px solid var(--border); font-size: 0.85rem; transition: all 0.15s; cursor: pointer; }
  .btn-primary { background: var(--accent); color: white; border-color: var(--accent); }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost { background: transparent; color: var(--fg-muted); }
  .btn-ghost:hover { background: var(--bg); color: var(--fg); }
  .btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-small { padding: 0.25rem 0.6rem; font-size: 0.75rem; }
  .feed-grid { margin-top: 0.5rem; }
  .feed-empty { text-align: center; padding: 3rem 0; color: var(--fg-muted); }
  .feed-empty-hint { font-size: 0.85rem; margin-top: 0.5rem; }
  .feed-end { display: flex; align-items: center; gap: 1rem; padding: 2rem 0; }
  .feed-end-line { flex: 1; height: 1px; background: var(--border); }
  .feed-end-text { margin: 0; font-size: 0.85rem; color: var(--fg-muted); white-space: nowrap; font-style: italic; }
</style>
