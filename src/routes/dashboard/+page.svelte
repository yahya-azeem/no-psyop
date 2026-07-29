<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import type { PostFilterResult } from '$lib/types';

  let textInput = $state('');
  let analysis = $state<PostFilterResult | null>(null);
  let analyzing = $state(false);

  let monitoredProfiles = $state<string[]>([]);
  let profileInput = $state('');
  let statusMsg = $state('');

  async function doAnalyze() {
    if (!textInput.trim()) return;
    analyzing = true;
    statusMsg = '';
    try {
      analysis = await invoke('analyze_post', { content: textInput });
    } catch (e) {
      statusMsg = `Analysis failed: ${e}`;
    } finally {
      analyzing = false;
    }
  }

  function addProfile() {
    const name = profileInput.trim();
    if (name && !monitoredProfiles.includes(name)) {
      monitoredProfiles = [...monitoredProfiles, name];
      profileInput = '';
      statusMsg = `Added ${name} to monitoring.`;
    }
  }

  function removeProfile(p: string) {
    monitoredProfiles = monitoredProfiles.filter(x => x !== p);
  }

  function analysisColor(): string {
    if (!analysis) return 'var(--fg-muted)';
    if (analysis.is_synthetic) return '#c44e4e';
    if (analysis.bait_score > 0.8) return '#b8860b';
    if (!analysis.should_filter) return '#2a7a4a';
    return '#b8860b';
  }

  function analysisLabel(): string {
    if (!analysis) return '';
    if (analysis.is_synthetic) return 'Synthetic / AI-generated';
    if (analysis.bait_score > 0.8) return 'Clickbait / Ragebait';
    if (!analysis.should_filter) return 'Quality content';
    return 'Low quality';
  }
</script>

<div class="dashboard-page">
  <div class="dashboard-header">
    <h2 class="dashboard-title">Intelligence Dashboard</h2>
    <p class="dashboard-subtitle">monitor content quality and track topics without doomscrolling</p>
  </div>

  {#if statusMsg}
    <div class="status-bar">{statusMsg}</div>
  {/if}

  <section class="dashboard-section">
    <h3 class="section-title">Content Analyzer</h3>
    <p class="section-desc">Paste any text to detect AI slop, clickbait, or synthetic content.</p>
    <div class="analyzer-box">
      <textarea
        bind:value={textInput}
        placeholder="Paste a post caption, headline, or article text…"
        rows="3"
      ></textarea>
      <button class="btn btn-primary" onclick={doAnalyze} disabled={analyzing || !textInput.trim()}>
        {analyzing ? 'Analyzing…' : 'Analyze'}
      </button>
    </div>

    {#if analysis}
      <div class="analysis-result" style="border-left-color: {analysisColor()}">
        <div class="analysis-badge" style="background: {analysisColor()}">
          {analysisLabel()}
        </div>
        <div class="analysis-stats">
          <span class="stat">
            Synthetic: <strong>{analysis.is_synthetic ? 'Yes' : 'No'}</strong>
          </span>
          <span class="stat">
            Bait score: <strong>{(analysis.bait_score * 100).toFixed(0)}%</strong>
          </span>
          <span class="stat">
            Quality: <strong>{!analysis.should_filter ? 'Yes' : 'No'}</strong>
          </span>
          <span class="stat">
            Filtered: <strong>{analysis.should_filter ? 'Yes' : 'No'}</strong>
          </span>
        </div>
      </div>
    {/if}
  </section>

  <section class="dashboard-section">
    <h3 class="section-title">Monitored Profiles</h3>
    <p class="section-desc">Track specific accounts for chronologically-bounded content.</p>
    <div class="profile-input">
      <input
        type="text"
        placeholder="username or profile URL"
        bind:value={profileInput}
        onkeydown={(e) => e.key === 'Enter' && addProfile()}
      />
      <button class="btn btn-primary" onclick={addProfile}>Add</button>
    </div>
    <div class="profile-list">
      {#each monitoredProfiles as profile}
        <div class="profile-chip">
          <span>{profile}</span>
          <button class="chip-remove" onclick={() => removeProfile(profile)}>×</button>
        </div>
      {:else}
        <p class="empty-hint">No profiles added yet. Add profiles to receive chronologically-bounded digests.</p>
      {/each}
    </div>
  </section>

  <section class="dashboard-section">
    <h3 class="section-title">Recent Digests</h3>
    <p class="section-desc">Topic-clustered summaries collected from your monitored profiles.</p>
    <p class="empty-hint">Digests will appear here after data is collected from monitored profiles.</p>
  </section>
</div>

<style>
  .dashboard-page { max-width: var(--max-width); }
  .dashboard-header { margin-bottom: 2rem; }
  .dashboard-title { font-size: 1.5rem; font-weight: 600; margin: 0; letter-spacing: -0.02em; }
  .dashboard-subtitle { font-size: 0.85rem; color: var(--fg-muted); margin: 0.25rem 0 0 0; }
  .status-bar { background: var(--bg); border: 1px solid var(--border); border-radius: var(--radius); padding: 0.5rem 0.75rem; font-size: 0.85rem; margin-bottom: 1rem; }
  .dashboard-section { margin-bottom: 2rem; }
  .section-title { font-size: 1rem; font-weight: 500; margin: 0 0 0.25rem 0; }
  .section-desc { font-size: 0.85rem; color: var(--fg-muted); margin: 0 0 0.75rem 0; }
  .analyzer-box { display: flex; flex-direction: column; gap: 0.5rem; }
  .analyzer-box textarea { width: 100%; padding: 0.6rem 0.75rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-card); color: var(--fg); font-size: 0.9rem; resize: vertical; }
  .btn { padding: 0.5rem 1rem; border-radius: var(--radius); border: 1px solid var(--border); font-size: 0.85rem; transition: all 0.15s; cursor: pointer; }
  .btn-primary { background: var(--accent); color: white; border-color: var(--accent); }
  .btn-primary:hover { background: var(--accent-hover); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .analysis-result { margin-top: 0.75rem; padding: 0.75rem 1rem; border: 1px solid var(--border); border-left: 4px solid; border-radius: var(--radius); background: var(--bg-card); }
  .analysis-badge { display: inline-block; color: white; font-size: 0.75rem; font-weight: 600; padding: 0.2rem 0.6rem; border-radius: 100px; margin-bottom: 0.5rem; text-transform: uppercase; letter-spacing: 0.05em; }
  .analysis-stats { display: flex; flex-wrap: wrap; gap: 1rem; font-size: 0.85rem; }
  .stat { color: var(--fg-muted); }
  .stat strong { color: var(--fg); }
  .profile-input { display: flex; gap: 0.5rem; margin-bottom: 0.75rem; }
  .profile-input input { flex: 1; padding: 0.5rem 0.75rem; border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-card); color: var(--fg); }
  .profile-list { display: flex; flex-wrap: wrap; gap: 0.5rem; }
  .profile-chip { display: flex; align-items: center; gap: 0.4rem; padding: 0.3rem 0.6rem; background: var(--bg); border: 1px solid var(--border); border-radius: 100px; font-size: 0.85rem; }
  .chip-remove { background: none; border: none; color: var(--fg-muted); font-size: 1rem; padding: 0; line-height: 1; cursor: pointer; }
  .chip-remove:hover { color: var(--danger); }
  .empty-hint { color: var(--fg-muted); font-size: 0.85rem; }
</style>
