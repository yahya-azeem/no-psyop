<script lang="ts">
  import type { ContentCluster } from '$lib/types';

  let clusters = $state<ContentCluster[]>([]);
  let profiles = $state<string[]>([]);
  let newProfile = $state('');

  function addProfile() {
    if (newProfile.trim() && !profiles.includes(newProfile.trim())) {
      profiles = [...profiles, newProfile.trim()];
      newProfile = '';
    }
  }

  function removeProfile(p: string) {
    profiles = profiles.filter(x => x !== p);
  }

  const topics = $derived(clusters.map(c => c.topic));
</script>

<div class="dashboard-page">
  <div class="dashboard-header">
    <h2 class="dashboard-title">Intelligence Dashboard</h2>
    <p class="dashboard-subtitle">monitor profiles and topics without doomscrolling</p>
  </div>

  <section class="dashboard-section">
    <h3 class="section-title">Monitored Profiles</h3>
    <div class="profile-input">
      <input
        type="text"
        placeholder="username or profile URL"
        bind:value={newProfile}
        onkeydown={(e) => e.key === 'Enter' && addProfile()}
      />
      <button class="btn btn-primary" onclick={addProfile}>Add</button>
    </div>

    <div class="profile-list">
      {#each profiles as profile}
        <div class="profile-chip">
          <span>{profile}</span>
          <button class="chip-remove" onclick={() => removeProfile(profile)}>×</button>
        </div>
      {:else}
        <p class="empty-hint">No profiles added yet.</p>
      {/each}
    </div>
  </section>

  <section class="dashboard-section">
    <h3 class="section-title">Recent Digests</h3>
    {#if clusters.length === 0}
      <p class="empty-hint">Digests will appear here after data is collected.</p>
    {/if}
  </section>
</div>

<style>
  .dashboard-page {
    max-width: var(--max-width);
  }

  .dashboard-header {
    margin-bottom: 2rem;
  }

  .dashboard-title {
    font-size: 1.5rem;
    font-weight: 600;
    margin: 0;
    letter-spacing: -0.02em;
  }

  .dashboard-subtitle {
    font-size: 0.85rem;
    color: var(--fg-muted);
    margin: 0.25rem 0 0 0;
  }

  .dashboard-section {
    margin-bottom: 2rem;
  }

  .section-title {
    font-size: 1rem;
    font-weight: 500;
    margin: 0 0 0.75rem 0;
  }

  .profile-input {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
  }

  .profile-input input {
    flex: 1;
    padding: 0.5rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg-card);
    color: var(--fg);
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

  .profile-list {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }

  .profile-chip {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.3rem 0.6rem;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 100px;
    font-size: 0.85rem;
  }

  .chip-remove {
    background: none;
    border: none;
    color: var(--fg-muted);
    font-size: 1rem;
    padding: 0;
    line-height: 1;
    cursor: pointer;
  }

  .chip-remove:hover {
    color: var(--danger);
  }

  .empty-hint {
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
</style>
