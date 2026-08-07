<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import MediaGrid from '$lib/components/MediaGrid.svelte';
  import type { FeedItem, Post } from '$lib/types';

  let platforms: { key: string; label: string }[] = [
    { key: '', label: 'All' },
    { key: 'Instagram', label: 'Instagram' },
    { key: 'Twitter', label: 'Twitter' },
    { key: 'LinkedIn', label: 'LinkedIn' },
  ];

  let selected = $state('');
  let query = $state('');
  let library = $state<FeedItem[]>([]);
  let live = $state<FeedItem[]>([]);
  let libraryLoading = $state(false);
  let liveLoading = $state(false);
  let error = $state('');

  function toItems(posts: Post[]): FeedItem[] {
    return posts.map((post) => ({
      post,
      proximity_score: 0,
      relevance_score: 1,
    }));
  }

  async function searchLibrary() {
    if (!query.trim()) return;
    libraryLoading = true;
    error = '';
    try {
      const posts = await invoke<Post[]>('search_library', {
        query: query.trim(),
        platform: selected || null,
      });
      library = toItems(posts);
    } catch (e) {
      error = `Library search failed: ${e}`;
    } finally {
      libraryLoading = false;
    }
  }

  async function searchLive() {
    if (!query.trim() || !selected) return;
    liveLoading = true;
    error = '';
    try {
      const posts = await invoke<Post[]>('search_platform', {
        platform: selected,
        query: query.trim(),
      });
      live = toItems(posts);
    } catch (e) {
      error = `Live search failed: ${e}`;
    } finally {
      liveLoading = false;
    }
  }

  function liveSupported(key: string): boolean {
    return key === 'Instagram' || key === 'Twitter';
  }
</script>

<div class="search-page">
  <div class="search-header">
    <h2 class="search-title">Search</h2>
    <p class="search-subtitle">find content across your indexed library, or query any platform live</p>
  </div>

  <div class="platform-tabs">
    {#each platforms as p (p.key)}
      <button
        class="tab"
        class:tab-active={selected === p.key}
        onclick={() => { selected = p.key; }}
      >
        {p.label}
      </button>
    {/each}
  </div>

  <div class="search-bar">
    <input
      type="text"
      placeholder="Search posts, captions, headlines…"
      bind:value={query}
      onkeydown={(e) => e.key === 'Enter' && searchLibrary()}
      class="search-input"
    />
    <button class="btn btn-primary" onclick={searchLibrary} disabled={libraryLoading || !query.trim()}>
      {libraryLoading ? '…' : 'Search library'}
    </button>
    {#if selected}
      <button
        class="btn btn-ghost"
        onclick={searchLive}
        disabled={liveLoading || !query.trim() || !liveSupported(selected)}
        title={liveSupported(selected) ? '' : 'LinkedIn uses the indexed library'}
      >
        {liveLoading ? '…' : 'Live search'}
      </button>
    {/if}
  </div>

  {#if error}
    <div class="status-bar">{error}</div>
  {/if}

  {#if selected && !liveSupported(selected)}
    <p class="hint">LinkedIn live scraping is unreliable, so its results come from your indexed library.</p>
  {/if}

  <details class="result-section" open>
    <summary class="result-summary">Library results ({library.length})</summary>
    {#if library.length > 0}
      <MediaGrid items={library} />
    {:else}
      <p class="empty-hint">Nothing in the library yet. Sync your platforms or try a live search.</p>
    {/if}
  </details>

  <details class="result-section" open>
    <summary class="result-summary">Live results ({live.length})</summary>
    {#if live.length > 0}
      <MediaGrid items={live} />
    {:else if liveLoading}
      <p class="empty-hint">Searching…</p>
    {:else}
      <p class="empty-hint">Run a live search to query a platform directly.</p>
    {/if}
  </details>

  {#if library.length === 0 && live.length === 0 && !libraryLoading && !liveLoading}
    <div class="no-results">
      <p>No results yet.</p>
      <p class="empty-hint">Start typing and hit "Search library", or pick a platform and "Live search".</p>
    </div>
  {/if}
</div>

<style>
  .search-page { max-width: var(--max-width); }
  .search-header { margin-bottom: 1.5rem; }
  .search-title { font-size: 1.5rem; font-weight: 600; margin: 0; letter-spacing: -0.02em; }
  .search-subtitle { font-size: 0.85rem; color: var(--fg-muted); margin: 0.25rem 0 0 0; }

  .platform-tabs { display: flex; gap: 0.5rem; margin-bottom: 1rem; flex-wrap: wrap; }
  .tab { padding: 0.4rem 0.9rem; border-radius: 100px; border: 1px solid var(--border); background: var(--bg-card); color: var(--fg-muted); font-size: 0.85rem; cursor: pointer; transition: all 0.15s; }
  .tab:hover { border-color: var(--accent); color: var(--fg); }
  .tab-active { background: var(--accent); border-color: var(--accent); color: white; }

  .search-bar { display: flex; gap: 0.5rem; margin-bottom: 1rem; flex-wrap: wrap; }
  .search-input { flex: 1; min-width: 200px; padding: 0.55rem 0.75rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-card); color: var(--fg); font-size: 0.9rem; }

  .btn { padding: 0.5rem 1rem; border-radius: var(--radius); border: 1px solid var(--border); font-size: 0.85rem; transition: all 0.15s; cursor: pointer; }
  .btn-primary { background: var(--accent); color: white; border-color: var(--accent); }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost { background: transparent; color: var(--fg-muted); }
  .btn-ghost:hover { background: var(--bg); color: var(--fg); }
  .btn-ghost:disabled { opacity: 0.4; cursor: not-allowed; }

  .status-bar { background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius); padding: 0.5rem 0.75rem; font-size: 0.85rem; margin-bottom: 1rem; }
  .hint { font-size: 0.8rem; color: var(--fg-muted); margin: 0 0 1rem 0; }

  .result-section { margin-bottom: 1.5rem; }
  .result-summary { cursor: pointer; font-size: 0.95rem; font-weight: 500; color: var(--fg); padding: 0.35rem 0; }
  .empty-hint { color: var(--fg-muted); font-size: 0.85rem; }
  .no-results { text-align: center; padding: 3rem 0; color: var(--fg-muted); }
</style>