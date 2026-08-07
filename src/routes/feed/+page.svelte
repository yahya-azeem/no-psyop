<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import MediaGrid from '$lib/components/MediaGrid.svelte';
  import NewsRow from '$lib/components/NewsRow.svelte';
  import StoriesTray from '$lib/components/StoriesTray.svelte';
  import { feed, isFetching, isCaughtUp, visiblePosts, addPosts, clearFeed } from '$lib/stores/feed';
  import { startupSync } from '$lib/autosync';
  import type { FeedItem, SyncResult } from '$lib/types';

  let items = $derived($visiblePosts);
  let syncing = $state(false);
  let syncMsg = $state('');

  let prevVersion = $state($startupSync.version);

  // Load the cached feed immediately so stored posts render before any sync
  // network work happens; the effect below then refreshes once a sync lands.
  onMount(() => {
    fetchFeed();
  });

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
      const result = await invoke<SyncResult>('sync_all', { force: true });
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

  <NewsRow />

  <div class="feed-grid">
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
  .feed-grid { margin-top: 0.5rem; }
  .feed-empty { text-align: center; padding: 3rem 0; color: var(--fg-muted); }
  .feed-empty-hint { font-size: 0.85rem; margin-top: 0.5rem; }
  .feed-end { display: flex; align-items: center; gap: 1rem; padding: 2rem 0; }
  .feed-end-line { flex: 1; height: 1px; background: var(--border); }
  .feed-end-text { margin: 0; font-size: 0.85rem; color: var(--fg-muted); white-space: nowrap; font-style: italic; }
</style>
