<script lang="ts">
  import VideoPlayer from '$lib/components/VideoPlayer.svelte';
  import { proxiedMedia } from '$lib/media';
  import { playWhenReady } from '$lib/video';
  import { invoke } from '@tauri-apps/api/core';
  import type { Comment, FeedItem, Post } from '$lib/types';

  let { items, sectioned = false }: { items: FeedItem[]; sectioned?: boolean } = $props();
  let openIdx = $state(-1);
  let slideIdx = $state(0);
  let comments = $state<Comment[]>([]);
  let commentsLoading = $state(false);
  let commentsError = $state('');
  let dismissed = $state(new Set<string>());
  let rssItems = $state<FeedItem[]>([]);
  let rssLoaded = $state(false);

  let catalog: FeedItem[] = $derived(sectioned ? [...items, ...rssItems] : items);
  let current = $derived(openIdx >= 0 && openIdx < catalog.length ? catalog[openIdx] : null);
  let visibleAll: FeedItem[] = $derived(items.filter((i) => !dismissed.has(i.post.id)));
  let gridItems: FeedItem[] = $derived(visibleAll.filter((i) => i.post.media_urls.length > 0));

  const VIRAL_THRESHOLD = 1000;

  async function loadRss() {
    if (rssLoaded) return;
    rssLoaded = true;
    try {
      await invoke<number>('rss_sync').catch(() => 0);
      const posts = await invoke<Post[]>('get_rss_posts');
      rssItems = posts.map((p) => ({ post: p, proximity_score: 0, relevance_score: 0 }));
    } catch {
      rssItems = [];
    }
  }

  $effect(() => {
    if (sectioned) loadRss();
  });

  function isFamily(item: FeedItem) {
    return item.post.author_is_mutual === true || item.post.author_is_close_friend === true;
  }

  function isColor(item: FeedItem) {
    return isFamily(item);
  }

  type Sec = { gi: number; item: FeedItem };
  let indexed: Sec[] = $derived(gridItems.map((item, gi) => ({ item, gi })));

  function isCorporate(s: { item: FeedItem }) {
    return s.item.post.platform === 'LinkedIn';
  }

  let corporateSec = $derived(
    visibleAll
      .filter(
        (i) =>
          isCorporate({ item: i }) &&
          (i.post.author_is_mutual === true || i.relevance_score > 0)
      )
      .map((item) => ({ item }))
  );
  let famSec = $derived(
    indexed.filter((s) => !isCorporate(s) && isFamily(s.item))
  );
  let viralSec = $derived(
    indexed.filter(
      (s) => !isCorporate(s) && !isFamily(s.item) && (s.item.post.engagement_score ?? 0) >= VIRAL_THRESHOLD
    )
  );
  let feedSec = $derived(
    indexed.filter(
      (s) => !isCorporate(s) && !isFamily(s.item) && (s.item.post.engagement_score ?? 0) < VIRAL_THRESHOLD
    )
  );
  let sections = $derived.by(() => {
    if (!sectioned) return [];
    const rssSec = rssItems.map((item) => ({ item }));
    return [
      { title: 'Feed', sub: 'posts from your networks', entries: feedSec },
      { title: 'Friends & Family', sub: 'mutuals and close friends', entries: famSec },
      { title: 'Viral for you', sub: 'high engagement', entries: viralSec },
      { title: 'Corporate', sub: 'from LinkedIn', entries: corporateSec },
      { title: 'RSS', sub: 'syndicated feeds', entries: rssSec },
    ].filter((s) => s.entries.length > 0);
  });

  async function loadComments(postId: string) {
    comments = [];
    commentsError = '';
    commentsLoading = true;
    try {
      comments = await invoke<Comment[]>('get_comments', { mediaId: postId });
    } catch (e) {
      commentsError = String(e);
    } finally {
      commentsLoading = false;
    }
  }

  $effect(() => {
    const c = current;
    if (c) {
      slideIdx = 0;
      loadComments(c.post.id);
    } else {
      comments = [];
      commentsError = '';
    }
  });

  function formatTimestamp(ts: number) {
    const d = new Date(ts * 1000);
    const now = Date.now();
    const diff = now - d.getTime();
    const hours = Math.floor(diff / 3600000);
    if (hours < 1) return 'just now';
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }

  function commentLabel(c: Comment) {
    if (c.author_username) return `@${c.author_username}`;
    if (c.author_id) return `@…${c.author_id.slice(-6)}`;
    return 'anonymous';
  }

  function playHover(e: MouseEvent) {
    const v = e.currentTarget as HTMLVideoElement;
    v.muted = true;
    playWhenReady(v);
  }

  function pauseHover(e: MouseEvent) {
    const v = e.currentTarget as HTMLVideoElement;
    v.pause();
  }

  function onMediaError(e: Event) {
    const el = e.currentTarget as HTMLElement;
    el.classList.add('media-error');
    el.style.display = 'none';
  }

  function open(i: number) {
    const it = gridItems[i];
    if (!it) return;
    openIdx = catalog.indexOf(it);
    dismissed.add(it.post.id);
    invoke('mark_post_seen', { platform: it.post.platform, postId: it.post.id }).catch(() => {});
  }

  function openItem(it: FeedItem) {
    if (!it) return;
    openIdx = catalog.indexOf(it);
    dismissed.add(it.post.id);
    invoke('mark_post_seen', { platform: it.post.platform, postId: it.post.id }).catch(() => {});
  }

  function next() {
    if (openIdx < catalog.length - 1) openIdx += 1;
  }

  function prev() {
    if (openIdx > 0) openIdx -= 1;
  }

  function close() {
    openIdx = -1;
  }

  function mediaCount(p: { media_urls: string[] }) {
    return p.media_urls.length;
  }

  function startsWithAt(u: string) {
    return u.startsWith('@') ? u : `@${u}`;
  }

  function nextSlide() {
    if (!current) return;
    const n = current.post.media_urls.length;
    if (n < 2) return;
    slideIdx = (slideIdx + 1) % n;
  }

  function prevSlide() {
    if (!current) return;
    const n = current.post.media_urls.length;
    if (n < 2) return;
    slideIdx = (slideIdx - 1 + n) % n;
  }
</script>

{#snippet cellBadge(item: FeedItem)}
  <span class="cell-meta">
    <span
      class="cell-logo"
      class:ig={(item.post.platform ?? '').toLowerCase().startsWith('insta')}
      class:tw={(item.post.platform ?? '').toLowerCase() === 'twitter' || (item.post.platform ?? '').toLowerCase().startsWith('x')}
      class:li={(item.post.platform ?? '').toLowerCase().startsWith('linked')}
    >
      {#if (item.post.platform ?? '').toLowerCase().indexOf('insta') === 0}
        <svg viewBox="0 0 24 24" width="12" height="12" fill="none" aria-hidden="true">
          <rect x="3" y="3" width="18" height="18" rx="5" stroke="currentColor" stroke-width="2"/>
          <circle cx="12" cy="12" r="4.2" stroke="currentColor" stroke-width="2"/>
          <circle cx="17.2" cy="6.8" r="1.3" fill="currentColor"/>
        </svg>
      {:else if (item.post.platform ?? '').toLowerCase() === 'twitter' || (item.post.platform ?? '').toLowerCase().startsWith('x')}
        <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor" aria-hidden="true">
          <path d="M18.9 1.15h3.68l-8.04 9.19L24 22.85h-7.41l-5.8-7.58-6.64 7.58H.47l8.6-9.83L0 1.15h7.59l5.24 6.93L18.9 1.15Zm-1.29 19.5h2.04L6.49 3.24H4.3l13.31 17.41Z"/>
        </svg>
      {:else if (item.post.platform ?? '').toLowerCase().startsWith('linked')}
        <svg viewBox="0 0 24 24" width="12" height="12" fill="currentColor" aria-hidden="true">
          <path d="M4.98 3.5A2.49 2.49 0 1 1 0 3.5a2.49 2.49 0 0 1 4.98 0ZM.24 8.31h4.52V24H.24V8.31ZM8.34 8.31h4.33v2.14h.06c.6-1.14 2.08-2.35 4.28-2.35 4.58 0 5.43 3.02 5.43 6.94V24h-4.52v-8.07c0-1.92-.04-4.4-2.68-4.4-2.68 0-3.09 2.09-3.09 4.26V24H8.34V8.31Z"/>
        </svg>
      {:else}
        <span class="cell-gen">◎</span>
      {/if}
    </span>
    {#if item.post.author_username}
      <span class="cell-user">{startsWithAt(item.post.author_username)}</span>
    {/if}
  </span>
{/snippet}

<div class="media-grid">
  {#if sectioned}
    {#each sections as sec}
      <section class="grid-section">
        <div class="section-head">
          <h3 class="section-title">{sec.title}</h3>
          <span class="section-sub">{sec.sub}</span>
        </div>
        <div class="grid-row">
          {#each sec.entries as { item }}
            <button
              class="grid-cell"
              class:is-video={item.post.is_video}
              class:color={isColor(item)}
              onclick={() => openItem(item)}
              aria-label={item.post.content || `${item.post.author_username} post`}
            >
              {#if item.post.media_urls[0]}
                {#if item.post.is_video}
                  <video
                    src={proxiedMedia(item.post.media_urls[0])}
                    poster={item.post.poster_url ? proxiedMedia(item.post.poster_url) : undefined}
                    muted
                    playsinline
                    preload="auto"
                    onmouseenter={playHover}
                    onmouseleave={pauseHover}
                    onerror={onMediaError}
                  ></video>
                  <span class="cell-badge">▶</span>
                {:else}
                  <img
                    src={proxiedMedia(item.post.media_urls[0])}
                    alt=""
                    loading="lazy"
                    onerror={onMediaError}
                  />
                  {#if item.post.media_urls.length > 1}
                    <span class="cell-count">{item.post.media_urls.length}</span>
                  {/if}
                {/if}
              {:else}
                <span class="cell-text">{item.post.content}</span>
              {/if}
              {#if isColor(item)}
                <span class="cell-friend">●</span>
              {/if}
              {@render cellBadge(item)}
            </button>
          {/each}
        </div>
      </section>
    {/each}
  {:else}
    {#each gridItems as item, i (item.post.id)}
      <button
        class="grid-cell"
        class:is-video={item.post.is_video}
        class:color={isColor(item)}
        onclick={() => open(i)}
        aria-label={item.post.content || `${item.post.author_username} post`}
      >
        {#if item.post.media_urls[0]}
          {#if item.post.is_video}
            <video
              src={proxiedMedia(item.post.media_urls[0])}
              poster={item.post.poster_url ? proxiedMedia(item.post.poster_url) : undefined}
              muted
              playsinline
              preload="auto"
              onmouseenter={playHover}
              onmouseleave={pauseHover}
              onerror={onMediaError}
            ></video>
            <span class="cell-badge">▶</span>
          {:else}
            <img
              src={proxiedMedia(item.post.media_urls[0])}
              alt=""
              loading="lazy"
              onerror={onMediaError}
            />
            {#if item.post.media_urls.length > 1}
              <span class="cell-count">{item.post.media_urls.length}</span>
            {/if}
          {/if}
        {:else}
          <span class="cell-text">{item.post.content}</span>
        {/if}
        {#if isColor(item)}
          <span class="cell-friend">●</span>
        {/if}
        {@render cellBadge(item)}
      </button>
    {/each}
  {/if}
</div>

{#if current}
  <div
    class="lightbox"
    role="button"
    tabindex="0"
    onclick={(e) => {
      if (e.target === e.currentTarget) close();
    }}
    onkeydown={(e) => {
      if (e.key === 'Escape') close();
      if (e.key === 'ArrowRight') next();
      if (e.key === 'ArrowLeft') prev();
    }}
  >
    <button class="lightbox-close" onclick={close} aria-label="close">×</button>

    {#if openIdx > 0}
      <button class="lightbox-nav prev" onclick={prev} aria-label="previous">‹</button>
    {/if}
    {#if openIdx < items.length - 1}
      <button class="lightbox-nav next" onclick={next} aria-label="next">›</button>
    {/if}

    <div class="lightbox-card">
      <div class="lightbox-media">
        {#if current.post.media_urls[0]}
          {#if current.post.is_video}
            <VideoPlayer src={proxiedMedia(current.post.media_urls[0])} autoplay />
          {:else if current.post.media_urls.length === 1}
            <img src={proxiedMedia(current.post.media_urls[0])} alt="" />
          {:else}
            <div class="slide-wrap">
              <img src={proxiedMedia(current.post.media_urls[slideIdx])} alt="" />
              <button class="slide-nav prev" onclick={prevSlide} aria-label="previous slide">‹</button>
              <button class="slide-nav next" onclick={nextSlide} aria-label="next slide">›</button>
              <div class="slide-dots">
                {#each current.post.media_urls as _, i}
                  <button
                    class="slide-dot"
                    class:active={slideIdx === i}
                    onclick={() => slideIdx = i}
                    aria-label={`slide ${i + 1}`}
                  ></button>
                {/each}
              </div>
            </div>
          {/if}
        {:else}
          <p class="lightbox-text">{current.post.content}</p>
        {/if}
      </div>
      <div class="lightbox-info">
        <div class="lightbox-header">
          <span class="lightbox-platform">{current.post.platform}</span>
          {#if current.post.platform === 'LinkedIn' && current.post.author_username}
            <a
              class="lightbox-author"
              href={`https://www.linkedin.com/in/${current.post.author_username}/`}
              target="_blank"
              rel="noopener noreferrer"
              onclick={(e) => e.stopPropagation()}
            >@{current.post.author_username}</a>
          {:else}
            <span class="lightbox-author">{current.post.author_username}</span>
          {/if}
          {#if current.post.author_is_close_friend}
            <span class="friend-badge close">close friend</span>
          {:else if current.post.author_is_mutual}
            <span class="friend-badge mutual">mutual</span>
          {/if}
          <span class="lightbox-time">{formatTimestamp(current.post.timestamp)}</span>
        </div>
        <p class="lightbox-content">{current.post.content}</p>
        <div class="lightbox-footer">
          <span>{(current.relevance_score * 100).toFixed(0)}% match</span>
          {#if current.proximity_score > 0}
            <span>{(current.proximity_score * 100).toFixed(0)}% proximity</span>
          {/if}
        </div>
        <div class="comments-section">
          <div class="comments-header">
            <span>Comments</span>
            {#if commentsLoading}
              <span class="comments-hint">loading…</span>
            {:else}
              <span class="comments-hint">{comments.length}</span>
            {/if}
          </div>
          {#if commentsError}
            <p class="comments-error">{commentsError}</p>
          {:else if comments.length === 0 && !commentsLoading}
            <p class="comments-empty">No comments yet.</p>
          {:else}
            <ul class="comments-list">
              {#each comments as c (c.id)}
                <li class="comment" class:own={c.is_mine}>
                  <span class="comment-author">{c.is_mine ? `${commentLabel(c)} (you)` : commentLabel(c)}</span>
                  <span class="comment-time">{formatTimestamp(c.timestamp)}</span>
                  <p class="comment-body">{c.content}</p>
                  {#if c.likes > 0}
                    <span class="comment-likes">♥ {c.likes}</span>
                  {/if}
                </li>
              {/each}
            </ul>
          {/if}
        </div>
        <span class="lightbox-count">{openIdx + 1} / {catalog.length}</span>
      </div>
    </div>
  </div>
{/if}

<style>
  .media-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.4rem;
  }
  .grid-section {
    grid-column: 1 / -1;
    display: contents;
  }
  .section-head {
    grid-column: 1 / -1;
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    padding: 1.1rem 0 0.4rem 0;
    border-top: 1px solid var(--border);
    margin-top: 0.5rem;
  }
  .grid-section:first-child .section-head {
    border-top: none;
    margin-top: 0;
    padding-top: 0.2rem;
  }
  .section-title {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 600;
    letter-spacing: -0.01em;
  }
  .section-sub {
    font-size: 0.75rem;
    color: var(--fg-muted);
  }
  .grid-row {
    grid-column: 1 / -1;
    display: contents;
  }
  .grid-cell {
    position: relative;
    aspect-ratio: 1 / 1;
    padding: 0;
    border: none;
    border-radius: var(--radius);
    overflow: hidden;
    background: var(--bg-card);
    filter: grayscale(100%);
    transition: filter 0.2s;
  }
  .grid-cell.color {
    filter: grayscale(0%);
  }
  .grid-cell:hover {
    filter: grayscale(0%);
  }
  .grid-cell :is(img, video) {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .cell-friend {
    position: absolute;
    top: 0.4rem;
    left: 0.4rem;
    width: 0.85rem;
    height: 0.85rem;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    font-size: 0.6rem;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    box-shadow: 0 0 0 2px rgba(0, 0, 0, 0.4);
  }
  .cell-text {
    position: absolute;
    inset: 0;
    padding: 0.5rem;
    padding-bottom: 1.6rem;
    font-size: 0.8rem;
    text-align: left;
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 6;
    line-clamp: 6;
  }
  .cell-meta {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.3rem 0.4rem;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.72), rgba(0, 0, 0, 0));
    pointer-events: none;
    color: #fff;
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.8);
  }
  .cell-logo {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.05rem;
    height: 1.05rem;
    flex: 0 0 auto;
    border-radius: 3px;
  }
  .cell-logo.ig {
    color: #e1306c;
    background: rgba(255, 255, 255, 0.92);
  }
  .cell-logo.tw {
    color: #000;
    background: #fff;
  }
  @media (prefers-color-scheme: light) {
    .cell-logo.tw { color: #111; }
  }
  .cell-logo.li {
    color: #0a66c2;
    background: rgba(255, 255, 255, 0.92);
  }
  .cell-gen {
    color: #fff;
    font-size: 0.7rem;
  }
  .cell-user {
    font-size: 0.66rem;
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .cell-badge {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 2rem;
    height: 2rem;
    border-radius: 50%;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.8rem;
    padding-left: 0.15rem;
    pointer-events: none;
  }
  .lightbox {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.88);
    z-index: 100;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .lightbox-card {
    display: flex;
    flex-direction: column;
    width: min(560px, 94vw);
    max-height: 90vh;
    background: var(--bg-card);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .lightbox-media {
    background: #000;
    flex: 0 0 auto;
  }
  .lightbox-media :is(img, :global(video)) {
    width: 100%;
    max-height: 55vh;
    object-fit: contain;
    display: block;
  }
  .slide-wrap {
    position: relative;
    width: 100%;
    max-height: 55vh;
  }
  .slide-wrap img {
    width: 100%;
    max-height: 55vh;
    object-fit: contain;
    display: block;
  }
  .slide-nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 2.2rem;
    height: 2.2rem;
    border-radius: 50%;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    font-size: 1.3rem;
    line-height: 1;
    cursor: pointer;
    z-index: 2;
  }
  .slide-nav.prev { left: 0.5rem; }
  .slide-nav.next { right: 0.5rem; }
  .slide-nav:hover { background: rgba(0, 0, 0, 0.8); }
  .slide-dots {
    position: absolute;
    bottom: 0.5rem;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    gap: 0.4rem;
    z-index: 2;
  }
  .slide-dot {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    border: 1px solid rgba(255, 255, 255, 0.7);
    background: transparent;
    cursor: pointer;
    padding: 0;
  }
  .slide-dot.active { background: #fff; }
  .cell-count {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    min-width: 1.4rem;
    padding: 0.1rem 0.35rem;
    border-radius: 999px;
    background: rgba(0, 0, 0, 0.6);
    color: #fff;
    font-size: 0.7rem;
    text-align: center;
    pointer-events: none;
  }
  .lightbox-text {
    padding: 2rem;
    margin: 0;
  }
  .lightbox-info {
    padding: 0.85rem 1rem;
    border-top: 1px solid var(--border);
    overflow-y: auto;
  }
  .lightbox-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
    margin-bottom: 0.5rem;
  }
  .lightbox-platform {
    font-weight: 600;
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--accent);
  }
  .lightbox-author {
    font-weight: 500;
  }
  .friend-badge {
    font-size: 0.65rem;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 600;
  }
  .friend-badge.mutual {
    background: rgba(84, 196, 128, 0.15);
    color: #54c480;
  }
  .friend-badge.close {
    background: rgba(232, 158, 60, 0.15);
    color: #e89e3c;
  }
  .lightbox-time {
    margin-left: auto;
    color: var(--fg-muted);
  }
  .lightbox-content {
    margin: 0 0 0.5rem 0;
    font-size: 0.9rem;
    line-height: 1.55;
    white-space: pre-wrap;
  }
  .lightbox-footer {
    display: flex;
    gap: 0.75rem;
    font-size: 0.78rem;
    color: var(--fg-muted);
  }
  .comments-section {
    margin-top: 0.75rem;
    border-top: 1px solid var(--border);
    padding-top: 0.6rem;
  }
  .comments-header {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    font-size: 0.78rem;
    font-weight: 600;
    margin-bottom: 0.4rem;
  }
  .comments-hint {
    color: var(--fg-muted);
    font-weight: 400;
  }
  .comments-empty,
  .comments-error {
    font-size: 0.8rem;
    color: var(--fg-muted);
    margin: 0.25rem 0;
  }
  .comments-error {
    color: #e5484d;
  }
  .comments-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
  }
  .comment { font-size: 0.82rem; line-height: 1.5; }
  .comment.own { color: var(--fg); }
  .comment-author {
    font-weight: 600;
    margin-right: 0.4rem;
  }
  .comment-time {
    font-size: 0.7rem;
    color: var(--fg-muted);
  }
  .comment-body {
    margin: 0.1rem 0 0 0;
    white-space: pre-wrap;
  }
  .comment-likes {
    font-size: 0.72rem;
    color: var(--fg-muted);
  }
  .lightbox-count {
    display: block;
    margin-top: 0.5rem;
    font-size: 0.75rem;
    color: var(--fg-muted);
  }
  .lightbox-close {
    position: absolute;
    top: 0.75rem;
    right: 0.75rem;
    z-index: 2;
    background: rgba(0, 0, 0, 0.5);
    color: #fff;
    border: none;
    border-radius: 50%;
    width: 2.2rem;
    height: 2.2rem;
    font-size: 1.3rem;
    line-height: 1;
    cursor: pointer;
  }
  .lightbox-nav {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    z-index: 2;
    background: rgba(0, 0, 0, 0.5);
    color: #fff;
    border: none;
    border-radius: 50%;
    width: 2.4rem;
    height: 2.4rem;
    font-size: 1.5rem;
    line-height: 1;
    cursor: pointer;
  }
  .lightbox-nav.prev {
    left: 1rem;
  }
  .lightbox-nav.next {
    right: 1rem;
  }
</style>
