import { invoke } from '@tauri-apps/api/core';
import { writable } from 'svelte/store';
import type { SyncResult } from '$lib/types';

export type StartupSyncStatus = 'idle' | 'syncing' | 'done' | 'error';

export const startupSync = writable<{ status: StartupSyncStatus; message: string; version: number }>({
  status: 'idle',
  message: '',
  version: 0,
});

let syncVersion = 0;

async function syncCycle(): Promise<void> {
  let creds: unknown[] = [];
  try {
    creds = await invoke('get_credentials');
  } catch {
    return;
  }
  if (creds.length === 0) return;

  startupSync.update((s) => ({ ...s, status: 'syncing', message: 'Syncing…' }));
  const errors: string[] = [];
  try {
    const result = await invoke<SyncResult>('sync_all');
    if (result.errors.length > 0) errors.push(...result.errors);
  } catch (e) {
    errors.push(String(e));
  }
  try {
    await invoke<number>('sync_messages', { platform: 'All' });
  } catch {
    // message sync is non-fatal
  }

  syncVersion += 1;
  if (errors.length > 0) {
    startupSync.set({ status: 'error', message: errors[0], version: syncVersion });
  } else {
    startupSync.set({ status: 'done', message: 'Synced', version: syncVersion });
  }
}

export async function runStartupSync(): Promise<void> {
  let creds: unknown[] = [];
  try {
    creds = await invoke('get_credentials');
  } catch {
    startupSync.set({ status: 'idle', message: '', version: syncVersion });
    return;
  }
  if (creds.length === 0) {
    startupSync.set({ status: 'idle', message: '', version: syncVersion });
    return;
  }
  await syncCycle();
}

let intervalId: ReturnType<typeof setInterval> | null = null;

export function startPeriodicSync(intervalMs = 5 * 60 * 1000): void {
  stopPeriodicSync();
  intervalId = setInterval(() => { syncCycle(); }, intervalMs);
}

export function stopPeriodicSync(): void {
  if (intervalId !== null) {
    clearInterval(intervalId);
    intervalId = null;
  }
}
