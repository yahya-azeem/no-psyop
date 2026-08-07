<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { Platform } from '$lib/types';

  interface PlatformAuth {
    platform: Platform;
    connected: boolean;
    username: string;
    sessionToken: string;
    loading: boolean;
  }

  let platforms = $state<PlatformAuth[]>([]);
  let statusMsg = $state('');

  async function loadCredentials() {
    const names: Platform[] = ['Instagram', 'Twitter', 'LinkedIn'];
    const creds: Record<string, { user_id: string; session_token: string }> = {};
    try {
      const list: { platform: string; user_id: string; session_token: string }[] = await invoke('get_credentials');
      for (const c of list) {
        creds[c.platform] = { user_id: c.user_id, session_token: c.session_token };
      }
    } catch {}
    platforms = names.map(name => ({
      platform: name,
      connected: !!creds[name],
      username: creds[name]?.user_id ?? '',
      sessionToken: '',
      loading: false,
    }));
  }

  onMount(loadCredentials);

  async function doConnect(p: PlatformAuth) {
    p.loading = true;
    statusMsg = '';
    try {
      await invoke('store_credential', {
        platform: p.platform,
        sessionToken: p.sessionToken,
        userId: p.username,
      });
      p.connected = true;
      p.sessionToken = '';
      statusMsg = `${p.platform} connected.`;
    } catch (e) {
      statusMsg = `Failed: ${e}`;
    } finally {
      p.loading = false;
    }
  }

  async function doBrowserConnect(p: PlatformAuth) {
    p.loading = true;
    statusMsg = '';
    try {
      statusMsg = 'Opening LinkedIn in a browser window — sign in there once.';
      await invoke('linkedin_connect');
      await loadCredentials();
      p.connected = true;
      statusMsg = 'LinkedIn connected.';
    } catch (e) {
      statusMsg = `Failed: ${e}`;
    } finally {
      p.loading = false;
    }
  }

  async function doDisconnect(p: PlatformAuth) {
    p.loading = true;
    statusMsg = '';
    try {
      await invoke('remove_credential', { platform: p.platform });
      p.connected = false;
      p.username = '';
      statusMsg = `${p.platform} disconnected.`;
    } catch (e) {
      statusMsg = `Failed: ${e}`;
    } finally {
      p.loading = false;
    }
  }
</script>

<div class="settings-page">
  <div class="settings-header">
    <h2 class="settings-title">Settings</h2>
    <p class="settings-subtitle">manage your connected accounts</p>
  </div>

  {#if statusMsg}
    <div class="status-bar">{statusMsg}</div>
  {/if}

  <section class="settings-section">
    <h3 class="section-title">Connected Platforms</h3>
    <p class="section-desc">Your session tokens are stored securely on-device (OS keychain).</p>

    <div class="platform-list">
      {#each platforms as p}
        <div class="platform-card">
          <div class="platform-info">
            <span class="platform-name">{p.platform}</span>
            <span class="platform-status" class:connected={p.connected}>
              {p.connected ? 'Connected' : 'Not connected'}
            </span>
          </div>
          <div class="platform-actions">
            {#if p.connected}
              <span class="username-label">{p.username}</span>
              <button class="btn btn-ghost" onclick={() => doDisconnect(p)} disabled={p.loading}>
                {p.loading ? '...' : 'Disconnect'}
              </button>
            {:else}
              {#if p.platform === 'LinkedIn'}
                <button class="btn btn-primary" onclick={() => doBrowserConnect(p)} disabled={p.loading}>
                  {p.loading ? 'Opening browser…' : 'Connect via browser'}
                </button>
              {:else}
                <input
                  type="text"
                  placeholder="username"
                  bind:value={p.username}
                  class="input-small"
                />
                <input
                  type="password"
                  placeholder="session_token"
                  bind:value={p.sessionToken}
                  class="input-small"
                />
                <button class="btn btn-primary" onclick={() => doConnect(p)} disabled={p.loading || !p.sessionToken}>
                  {p.loading ? '...' : 'Connect'}
                </button>
              {/if}
            {/if}
          </div>
        </div>
      {/each}
    </div>
  </section>

  <section class="settings-section">
    <h3 class="section-title">Privacy</h3>
    <p class="section-desc">All data stays on this device. No servers.</p>
    <div class="privacy-info">
      <p>Biometric-bound token storage via OS keyring</p>
      <p>Zero server-side footprint</p>
      <p>On-device heuristic AI (no model downloads)</p>
    </div>
  </section>
</div>

<style>
  .settings-page { max-width: var(--max-width); }
  .settings-header { margin-bottom: 2rem; }
  .settings-title { font-size: 1.5rem; font-weight: 600; margin: 0; letter-spacing: -0.02em; }
  .settings-subtitle { font-size: 0.85rem; color: var(--fg-muted); margin: 0.25rem 0 0 0; }
  .status-bar { background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius); padding: 0.5rem 0.75rem; font-size: 0.85rem; margin-bottom: 1rem; }
  .settings-section { margin-bottom: 2rem; }
  .section-title { font-size: 1rem; font-weight: 500; margin: 0 0 0.25rem 0; }
  .section-desc { font-size: 0.85rem; color: var(--fg-muted); margin: 0 0 1rem 0; }
  .platform-list { display: flex; flex-direction: column; gap: 0.75rem; }
  .platform-card { display: flex; align-items: center; justify-content: space-between; gap: 1rem; padding: 0.75rem 1rem; background: var(--bg-card); border: 1px solid var(--border); border-radius: var(--radius); flex-wrap: wrap; }
  .platform-info { display: flex; flex-direction: column; gap: 0.15rem; }
  .platform-name { font-weight: 500; }
  .platform-status { font-size: 0.8rem; color: var(--fg-muted); }
  .platform-status.connected { color: #2a7a4a; }
  .platform-actions { display: flex; align-items: center; gap: 0.5rem; flex-wrap: wrap; }
  .username-label { font-size: 0.85rem; color: var(--fg-muted); }
  .input-small { padding: 0.35rem 0.6rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg); color: var(--fg); width: 130px; font-size: 0.85rem; }
  .btn { padding: 0.4rem 0.85rem; border-radius: var(--radius); border: 1px solid var(--border); font-size: 0.8rem; transition: all 0.15s; white-space: nowrap; cursor: pointer; }
  .btn-primary { background: var(--accent); color: white; border-color: var(--accent); }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost { background: transparent; color: var(--fg-muted); }
  .btn-ghost:hover { background: var(--bg); color: var(--danger); }
  .btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }
  .privacy-info p { margin: 0.35rem 0; font-size: 0.9rem; }
</style>
