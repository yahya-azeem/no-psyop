<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { startupSync } from '$lib/autosync';
  import type { Conversation, Message } from '$lib/types';

  let conversations = $state<Conversation[]>([]);
  let selectedConv = $state<string | null>(null);
  let selectedPlatform = $state<string>('');
  let messages = $state<Message[]>([]);
  let loading = $state(false);
  let syncingMsgs = $state(false);
  let syncMsg = $state('');
  let reply = $state('');
  let sending = $state(false);
  let sendMsg = $state('');
  let prevVersion = $state($startupSync.version);

  async function loadConversations() {
    loading = true;
    try {
      conversations = await invoke('get_conversations', { platform: null });
    } catch {
      conversations = [];
    } finally {
      loading = false;
    }
  }

  async function syncMessages() {
    syncingMsgs = true;
    syncMsg = '';
    try {
      const saved = await invoke<number>('sync_messages', { platform: 'All', force: true });
      syncMsg = `Synced ${saved} new messages.`;
    } catch (e) {
      syncMsg = `Sync failed: ${e}`;
    } finally {
      syncingMsgs = false;
      await loadConversations();
    }
  }

  $effect(() => {
    const v = $startupSync.version;
    if (v > 0 && v !== prevVersion) {
      loadConversations();
    }
    prevVersion = v;
  });

  async function selectConversation(conv: Conversation) {
    selectedConv = conv.id;
    selectedPlatform = conv.platform;
    sendMsg = '';
    try {
      messages = await invoke('get_messages', {
        conversationId: conv.id,
        platform: conv.platform,
      });
    } catch {
      messages = [];
    }
  }

  async function sendReply() {
    const content = reply.trim();
    if (!selectedConv || !content) return;
    sending = true;
    sendMsg = '';
    try {
      await invoke('send_message', {
        platform: selectedPlatform,
        conversationId: selectedConv,
        content,
      });
      reply = '';
      sendMsg = 'Sent.';
      try {
        messages = await invoke('get_messages', {
          conversationId: selectedConv,
          platform: selectedPlatform,
        });
      } catch {
        /* keep current list */
      }
    } catch (e) {
      sendMsg = `Send failed: ${e}`;
    } finally {
      sending = false;
    }
  }

  function platformColor(p: string): string {
    if (p === 'Instagram') return '#c13584';
    if (p === 'Twitter') return '#1da1f2';
    if (p === 'LinkedIn') return '#0a66c2';
    return 'var(--fg-muted)';
  }

  function formatTimestamp(ts: number) {
    const d = new Date(ts * 1000);
    const now = Date.now();
    const diff = now - d.getTime();
    const hours = Math.floor(diff / 3600000);
    if (hours < 1) return 'just now';
    if (hours < 24) return `${hours}h ago`;
    return `${Math.floor(hours / 24)}d ago`;
  }

  onMount(loadConversations);
</script>

<div class="inbox-page">
  <div class="inbox-header">
    <h2 class="inbox-title">Inbox</h2>
    <p class="inbox-subtitle">unified messages from all networks</p>
  </div>

  <div class="inbox-toolbar">
    <button class="btn btn-primary" onclick={syncMessages} disabled={syncingMsgs}>
      {syncingMsgs ? 'Syncing…' : 'Sync messages'}
    </button>
    <button class="btn btn-ghost" onclick={loadConversations} disabled={loading}>
      {loading ? '...' : 'Refresh'}
    </button>
    {#if syncMsg}
      <span class="sync-msg">{syncMsg}</span>
    {/if}
  </div>

  <div class="inbox-layout">
    <div class="conv-list">
      {#if conversations.length === 0}
        <p class="empty-hint">No conversations yet.</p>
      {/if}
      {#each conversations as conv}
        <div
          class="conv-item"
          class:selected={selectedConv === conv.id}
          onclick={() => selectConversation(conv)}
          role="button"
          tabindex="0"
          onkeydown={(e) => e.key === 'Enter' && selectConversation(conv)}
        >
          <div class="conv-info">
            <div class="conv-top">
              <span class="platform-badge" style="color: {platformColor(conv.platform)}">{conv.platform}</span>
              <span class="conv-participants">{conv.participants.join(', ')}</span>
            </div>
            <span class="conv-time">{formatTimestamp(conv.last_message_at)}</span>
          </div>
          {#if conv.unread}
            <span class="unread-badge">●</span>
          {/if}
        </div>
      {/each}
    </div>

    <div class="msg-area">
      {#if selectedConv}
        <div class="msg-list">
          {#each messages as msg}
            <div
              class="msg-bubble"
              class:own={msg.is_mine || msg.sender_id === 'You' || msg.sender_id === ''}
            >
              <div class="msg-sender">
                <span class="platform-badge small" style="color: {platformColor(msg.platform)}">{msg.platform}</span>
                <span class="msg-sender-name">{msg.sender_id === 'You' || msg.sender_id === '' ? 'You' : msg.sender_id}</span>
              </div>
              <div class="msg-content">{msg.content}</div>
              <div class="msg-time">{formatTimestamp(msg.timestamp)}</div>
            </div>
          {/each}
        </div>
        <div class="composer">
          <textarea
            class="composer-input"
            placeholder={`Reply on ${selectedPlatform}…`}
            bind:value={reply}
            rows="2"
            onkeydown={(e) => {
              if (e.key === 'Enter' && !e.shiftKey) {
                e.preventDefault();
                sendReply();
              }
            }}
          ></textarea>
          <div class="composer-bar">
            <button class="btn btn-primary" onclick={sendReply} disabled={sending || !reply.trim()}>
              {sending ? 'Sending…' : 'Send'}
            </button>
            {#if sendMsg}
              <span class="sync-msg">{sendMsg}</span>
            {/if}
          </div>
        </div>
      {:else}
        <p class="empty-hint">Select a conversation to view messages.</p>
      {/if}
    </div>
  </div>
</div>

<style>
  .inbox-page { max-width: var(--max-width); }
  .inbox-header { margin-bottom: 1.5rem; }
  .inbox-title { font-size: 1.5rem; font-weight: 600; margin: 0; letter-spacing: -0.02em; }
  .inbox-subtitle { font-size: 0.85rem; color: var(--fg-muted); margin: 0.25rem 0 0 0; }
  .inbox-toolbar { display: flex; gap: 0.5rem; margin-bottom: 1rem; align-items: center; }
  .btn { padding: 0.4rem 0.85rem; border-radius: var(--radius); border: 1px solid var(--border); font-size: 0.8rem; transition: all 0.15s; cursor: pointer; }
  .btn-primary { background: var(--accent); color: white; border-color: var(--accent); }
  .btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-ghost { background: transparent; color: var(--fg-muted); }
  .btn-ghost:hover { background: var(--bg); color: var(--fg); }
  .btn-ghost:disabled { opacity: 0.5; cursor: not-allowed; }
  .sync-msg { font-size: 0.8rem; color: var(--fg-muted); }
  .inbox-layout { display: grid; grid-template-columns: 260px 1fr; gap: 1rem; min-height: 400px; }
  .conv-list { border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-card); padding: 0.5rem; max-height: 500px; overflow-y: auto; }
  .conv-item { display: flex; align-items: center; justify-content: space-between; padding: 0.5rem 0.75rem; border-radius: var(--radius); cursor: pointer; transition: background 0.1s; }
  .conv-item:hover { background: var(--bg); }
  .conv-item.selected { background: var(--bg); font-weight: 500; }
  .conv-info { display: flex; flex-direction: column; gap: 0.15rem; }
  .conv-top { display: flex; align-items: center; gap: 0.4rem; }
  .platform-badge { font-size: 0.65rem; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em; }
  .platform-badge.small { font-size: 0.6rem; margin-right: 0.3rem; }
  .conv-participants { font-size: 0.85rem; }
  .conv-time { font-size: 0.75rem; color: var(--fg-muted); }
  .unread-badge { color: var(--accent); font-size: 0.75rem; }
  .msg-area { border: 1px solid var(--border); border-radius: var(--radius); background: var(--bg-card); padding: 1rem; max-height: 500px; overflow-y: auto; }
  .msg-list { display: flex; flex-direction: column; gap: 0.75rem; }
  .msg-bubble { padding: 0.5rem 0.75rem; background: var(--bg); border-radius: var(--radius); }
  .msg-bubble.own { align-self: flex-end; background: var(--accent); color: white; }
  .msg-bubble.own .msg-sender-name { color: rgba(255, 255, 255, 0.85); }
  .msg-bubble.own .msg-time { color: rgba(255, 255, 255, 0.7); }
  .msg-sender { font-size: 0.8rem; font-weight: 500; margin-bottom: 0.25rem; display: flex; align-items: baseline; }
  .msg-sender-name { font-weight: 700; }
  .msg-content { font-size: 0.9rem; line-height: 1.5; }
  .msg-time { font-size: 0.7rem; color: var(--fg-muted); margin-top: 0.25rem; }
  .composer { margin-top: 0.75rem; border-top: 1px solid var(--border); padding-top: 0.6rem; }
  .composer-input { width: 100%; box-sizing: border-box; background: var(--bg); color: var(--fg); border: 1px solid var(--border); border-radius: var(--radius); padding: 0.5rem 0.65rem; font: inherit; resize: vertical; }
  .composer-input:focus { outline: none; border-color: var(--accent); }
  .composer-bar { display: flex; align-items: center; gap: 0.5rem; margin-top: 0.5rem; }
  .empty-hint { color: var(--fg-muted); font-size: 0.85rem; text-align: center; padding: 2rem 0; }
</style>
