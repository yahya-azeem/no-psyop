import { writable, derived } from 'svelte/store';
import type { FeedItem, Post } from '$lib/types';

export const feed = writable<FeedItem[]>([]);
export const isFetching = writable(false);
export const isCaughtUp = writable(false);

export const feedCount = derived(feed, ($feed) => $feed.length);

export const visiblePosts = derived(feed, ($feed) =>
  $feed.filter((item) => !item.post.is_synthetic)
);

export function addPosts(items: FeedItem[]) {
  feed.update((current) => {
    const existing = new Set(current.map((i) => i.post.id));
    const new_items = items.filter((i) => !existing.has(i.post.id));
    return [...current, ...new_items];
  });
}

export function clearFeed() {
  feed.set([]);
  isCaughtUp.set(false);
}
