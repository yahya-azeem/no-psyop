<script lang="ts">
  import { onMount } from 'svelte';

  let { src, poster = '', loop = true, showControls = false, autoplay = false } = $props();

  let videoEl: HTMLVideoElement = $state() as HTMLVideoElement;

  function playMuted() {
    if (!videoEl) return;
    videoEl.muted = true;
    videoEl.play().catch(() => {});
  }

  function pause() {
    if (!videoEl) return;
    videoEl.pause();
  }

  function togglePlay() {
    if (!videoEl) return;
    if (videoEl.paused) {
      videoEl.muted = false;
      videoEl.play().catch(() => {});
    } else {
      videoEl.pause();
    }
  }

  onMount(() => {
    if (autoplay) playMuted();
  });
</script>

<video
  bind:this={videoEl}
  class="video-player"
  src={src}
  {poster}
  {loop}
  playsinline
  muted
  controls={showControls}
  preload="metadata"
  onmouseenter={playMuted}
  onmouseleave={pause}
  onclick={togglePlay}
></video>

<style>
  .video-player {
    width: 100%;
    display: block;
    background: #000;
    cursor: pointer;
  }
</style>
