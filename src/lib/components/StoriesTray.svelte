<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount, onDestroy } from 'svelte';
  import { proxiedMedia } from '$lib/media';
  import type { StoryUser } from '$lib/types';

  let stories = $state<StoryUser[]>([]);
  let loading = $state(false);
  let error = $state('');
  let viewerOpen = $state(false);
  let userIdx = $state(0);
  let itemIdx = $state(0);
  let progress = $state(0);
  let timerId: ReturnType<typeof setTimeout> | null = null;

  let currentUser = $derived(viewerOpen ? stories[userIdx] : null);
  let currentItem = $derived(currentUser?.items[itemIdx] ?? null);

  async function load() {
    loading = true;
    error = '';
    try {
      const list = await invoke<StoryUser[]>('get_stories');
      stories = list.filter((s) => s.items.length > 0);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  function openViewer(i: number) {
    userIdx = i;
    itemIdx = 0;
    progress = 0;
    viewerOpen = true;
    startTimer();
  }

  function closeViewer() {
    viewerOpen = false;
    clearTimer();
  }

  function clearTimer() {
    if (timerId) {
      clearTimeout(timerId);
      timerId = null;
    }
  }

  function startTimer() {
    clearTimer();
    if (!currentItem) return;
    const duration = currentItem.is_video ? 8000 : 5000;
    const tickMs = 50;
    let p = progress;
    timerId = setTimeout(function tick() {
      p += tickMs / duration;
      progress = p;
      if (p >= 1) {
        nextItem();
      } else {
        timerId = setTimeout(tick, tickMs);
      }
    }, tickMs);
  }

  function nextItem() {
    if (!currentUser) return;
    if (itemIdx < currentUser.items.length - 1) {
      itemIdx += 1;
      progress = 0;
      startTimer();
    } else if (userIdx < stories.length - 1) {
      userIdx += 1;
      itemIdx = 0;
      progress = 0;
      startTimer();
    } else {
      closeViewer();
    }
  }

  function prevItem() {
    if (!currentUser) return;
    if (itemIdx > 0) {
      itemIdx -= 1;
      progress = 0;
      startTimer();
    } else if (userIdx > 0) {
      userIdx -= 1;
      itemIdx = stories[userIdx].items.length - 1;
      progress = 0;
      startTimer();
    }
  }

  function onVideoEnded() {
    nextItem();
  }

  function onStoryPlay(e: Event) {
    const v = e.currentTarget as HTMLVideoElement;
    setTimeout(() => { v.muted = false; }, 100);
  }

  onDestroy(clearTimer);
</script>

<section class="stories-section">
  <div class="stories-header">
    <h3 class="stories-title">Stories</h3>
    <button class="btn btn-ghost btn-small" onclick={load} disabled={loading}>
      {loading ? '…' : 'Refresh'}
    </button>
  </div>

  {#if error}
    <p class="stories-error">{error}</p>
  {/if}

  {#if stories.length === 0 && !loading && !error}
    <p class="stories-empty">No active stories. Connect Instagram and refresh.</p>
  {/if}

  <div class="story-tray">
    {#each stories as story, i (story.id)}
      <button
        class="story-avatar"
        class:friend={story.is_mutual || story.is_close_friend}
        class:close={story.is_close_friend}
        onclick={() => openViewer(i)}
        title={story.username}
      >
        {#if story.profile_pic_url}
          <img src={proxiedMedia(story.profile_pic_url)} alt={story.username} loading="lazy" />
        {:else}
          <span class="story-avatar-fallback">{story.username[0]?.toUpperCase() ?? '?'}</span>
        {/if}
        <span class="story-name">{story.username}</span>
      </button>
    {/each}
  </div>
</section>

{#if viewerOpen && currentUser && currentItem}
  <div
    class="story-viewer"
    role="button"
    tabindex="0"
    onclick={(e) => {
      if (e.target === e.currentTarget) closeViewer();
    }}
    onkeydown={(e) => {
      if (e.key === 'Escape') closeViewer();
      if (e.key === 'ArrowRight') { e.stopPropagation(); nextItem(); }
      if (e.key === 'ArrowLeft') { e.stopPropagation(); prevItem(); }
    }}
  >
    <div class="story-viewer-inner">
      <div class="story-progress">
        {#each currentUser.items as item, i (item.id)}
          <div class="progress-bar">
            <div
              class="progress-fill"
              style="transform: scaleX({i < itemIdx ? 1 : i === itemIdx ? progress : 0})"
            ></div>
          </div>
        {/each}
      </div>

      <div class="story-topbar">
        <span class="story-username">{currentUser.username}</span>
        <button class="story-close" onclick={closeViewer}>×</button>
      </div>

      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="story-media" onmouseenter={clearTimer} onmouseleave={startTimer}>
        {#if currentItem.is_video}
          <!-- svelte-ignore a11y_media_has_caption -->
          <video
            src={proxiedMedia(currentItem.media_url)}
            poster={currentItem.poster_url ? proxiedMedia(currentItem.poster_url) : undefined}
            autoplay
            muted
            playsinline
            onended={onVideoEnded}
            onplay={onStoryPlay}
          ></video>
        {:else}
          <img src={proxiedMedia(currentItem.media_url)} alt="" />
        {/if}
      </div>

      <button class="story-nav prev" onclick={prevItem} aria-label="previous">‹</button>
      <button class="story-nav next" onclick={nextItem} aria-label="next">›</button>
    </div>
  </div>
{/if}

<style>
  .stories-section { margin-bottom: 1.5rem; }
  .stories-header { display: flex; align-items: center; gap: 0.75rem; margin-bottom: 0.75rem; }
  .stories-title { font-size: 1rem; font-weight: 500; margin: 0; }
  .stories-error { color: var(--danger, #c44e4e); font-size: 0.85rem; }
  .stories-empty { color: var(--fg-muted); font-size: 0.85rem; }
  .story-tray { display: flex; gap: 0.9rem; overflow-x: auto; padding-bottom: 0.5rem; }
  .story-avatar { display: flex; flex-direction: column; align-items: center; gap: 0.3rem; background: none; border: none; cursor: pointer; width: 72px; }
  .story-avatar img, .story-avatar-fallback {
    width: 56px; height: 56px; border-radius: 50%;
    border: 2px solid var(--accent); object-fit: cover; background: var(--bg-card);
  }
  .story-avatar.friend img, .story-avatar.friend .story-avatar-fallback {
    border-color: #54c480;
  }
  .story-avatar.close img, .story-avatar.close .story-avatar-fallback {
    border-color: #e89e3c;
  }
  .story-avatar-fallback { display: flex; align-items: center; justify-content: center; color: var(--fg); font-weight: 600; }
  .story-name { font-size: 0.7rem; color: var(--fg-muted); max-width: 72px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .story-viewer {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.92); z-index: 100;
    display: flex; align-items: center; justify-content: center;
  }
  .story-viewer-inner { position: relative; width: min(420px, 100vw); height: min(80vh, 100vw); }
  .story-progress { position: absolute; top: 0; left: 0; right: 0; display: flex; gap: 0.25rem; z-index: 2; padding: 0.5rem; }
  .progress-bar { flex: 1; height: 3px; background: rgba(255, 255, 255, 0.3); border-radius: 2px; overflow: hidden; }
  .progress-fill { height: 100%; background: #fff; transform-origin: left; transition: transform 0.05s linear; }
  .story-topbar { position: absolute; top: 0.75rem; left: 0; right: 0; display: flex; align-items: center; justify-content: space-between; padding: 0.5rem 1rem; z-index: 2; }
  .story-username { color: #fff; font-weight: 600; font-size: 0.9rem; text-shadow: 0 1px 2px rgba(0,0,0,0.6); }
  .story-close { background: none; border: none; color: #fff; font-size: 1.6rem; line-height: 1; cursor: pointer; }
  .story-media { width: 100%; height: 100%; border-radius: 12px; overflow: hidden; background: #000; }
  .story-media :is(img, video) { width: 100%; height: 100%; object-fit: contain; }
  .story-nav {
    position: absolute; top: 50%; transform: translateY(-50%); z-index: 3;
    background: rgba(0, 0, 0, 0.5); color: #fff; border: none; border-radius: 50%;
    width: 2.2rem; height: 2.2rem; font-size: 1.4rem; line-height: 1; cursor: pointer;
  }
  .story-nav.prev { left: -0.5rem; }
  .story-nav.next { right: -0.5rem; }
</style>
