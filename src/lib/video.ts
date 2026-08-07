export function playWhenReady(v: HTMLVideoElement): void {
  if (!v) return;
  // Nudge the element to actually pull the stream (not just metadata).
  if (v.preload !== 'auto') v.preload = 'auto';
  const start = () => {
    if (v.paused && v.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA) {
      v.play().catch(() => {});
    }
  };
  if (v.readyState >= HTMLMediaElement.HAVE_FUTURE_DATA) {
    start();
  } else {
    v.addEventListener('canplay', start, { once: true });
    v.addEventListener('loadeddata', start, { once: true });
  }
}