<script lang="ts">
  import '../app.css';
  import { onMount, onDestroy } from 'svelte';
  import { runStartupSync, startPeriodicSync, stopPeriodicSync, startupSync } from '$lib/autosync';
  import { registerPwa } from '$lib/pwa';

  let { children } = $props();
  let active = $state('feed');

  onMount(() => {
    registerPwa();

    // Defer the first sync so the cached feed paints before the browser work
    // for news + feeds kicks off — avoids a CPU spike right at first render.
    const t = setTimeout(() => runStartupSync(), 1500);
    startPeriodicSync();
    return () => {
      clearTimeout(t);
      stopPeriodicSync();
    };
  });

  onDestroy(() => {
    stopPeriodicSync();
  });
</script>

<div class="app-shell">
  <aside class="sidebar">
    <div class="sidebar-header">
      <h1 class="brand">OneMedia</h1>
      <p class="tagline">curated. finite. yours.</p>
    </div>

    <nav class="sidebar-nav">
      <a href="/feed" class="nav-item" class:active={active === 'feed'}
         onclick={() => active = 'feed'}>
        <span class="nav-icon">≡</span>
        <span>Feed</span>
      </a>

      <a href="/inbox" class="nav-item" class:active={active === 'inbox'}
         onclick={() => active = 'inbox'}>
        <span class="nav-icon">✉</span>
        <span>Inbox</span>
      </a>

      <a href="/search" class="nav-item" class:active={active === 'search'}
         onclick={() => active = 'search'}>
        <span class="nav-icon">⌕</span>
        <span>Search</span>
      </a>

      <a href="/dashboard" class="nav-item" class:active={active === 'dashboard'}
         onclick={() => active = 'dashboard'}>
        <span class="nav-icon">◇</span>
        <span>Dashboard</span>
      </a>

      <a href="/settings" class="nav-item" class:active={active === 'settings'}
         onclick={() => active = 'settings'}>
        <span class="nav-icon">⚙</span>
        <span>Settings</span>
      </a>
    </nav>

    <div class="sidebar-footer">
      {#if $startupSync.status !== 'idle'}
        <span class="sync-pill" class:done={$startupSync.status === 'done'}
              class:error={$startupSync.status === 'error'}>
          {#if $startupSync.status === 'syncing'}
            <span class="sync-spinner"></span>
          {/if}
          {$startupSync.message}
        </span>
      {/if}
      <span class="version">v0.1.0</span>
    </div>
  </aside>

  <main class="main-content">
    {@render children()}
  </main>
</div>

<style>
  .app-shell {
    display: flex;
    min-height: 100vh;
  }

  .sidebar {
    width: 220px;
    background: var(--bg-card);
    border-right: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    padding: 1.5rem 1rem;
    position: sticky;
    top: 0;
    height: 100vh;
  }

  .sidebar-header {
    margin-bottom: 2rem;
  }

  .brand {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0;
    letter-spacing: -0.02em;
    text-transform: lowercase;
  }

  .tagline {
    font-size: 0.75rem;
    color: var(--fg-muted);
    margin: 0.25rem 0 0 0;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .sidebar-nav {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    flex: 1;
  }

  .nav-item {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.5rem 0.75rem;
    border-radius: var(--radius);
    color: var(--fg);
    font-size: 0.9rem;
    transition: background 0.15s;
    text-decoration: none;
  }

  .nav-item:hover {
    background: var(--bg);
    text-decoration: none;
  }

  .nav-item.active {
    background: var(--bg);
    font-weight: 500;
  }

  .nav-icon {
    width: 1.25rem;
    text-align: center;
    opacity: 0.6;
  }

  .sidebar-footer {
    padding-top: 1rem;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .sync-pill {
    font-size: 0.7rem;
    color: var(--fg-muted);
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }

  .sync-pill.done {
    color: var(--accent);
  }

  .sync-pill.error {
    color: #e5484d;
  }

  .sync-spinner {
    width: 0.7rem;
    height: 0.7rem;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .version {
    font-size: 0.75rem;
    color: var(--fg-muted);
  }

  .main-content {
    flex: 1;
    max-width: var(--max-width);
    padding: 2rem 1.5rem;
  }

  @media (max-width: 680px) {
    .app-shell {
      flex-direction: column;
    }

    /* Sidebar becomes a fixed bottom tab bar on phones. */
    .sidebar {
      position: fixed;
      left: 0;
      right: 0;
      bottom: 0;
      top: auto;
      width: 100%;
      height: auto;
      flex-direction: row;
      padding: env(safe-area-inset-top) 0;
      padding-bottom: calc(0.35rem + env(safe-area-inset-bottom));
      border-right: none;
      border-top: 1px solid var(--border);
      border-bottom: none;
      background: var(--bg-card);
      z-index: 50;
    }
    .sidebar-header, .sidebar-footer {
      display: none;
    }
    .sidebar-nav {
      flex-direction: row;
      flex: 1;
      justify-content: space-around;
      gap: 0;
    }
    .nav-item {
      flex-direction: column;
      gap: 0.2rem;
      padding: 0.5rem 0.25rem;
      font-size: 0.7rem;
    }
    .nav-icon {
      width: auto;
      font-size: 1.2rem;
      line-height: 1;
    }
    .main-content {
      padding: 1rem;
      padding-bottom: calc(4.6rem + env(safe-area-inset-bottom));
    }
  }
</style>
