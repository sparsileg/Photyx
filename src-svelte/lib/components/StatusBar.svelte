<!-- StatusBar.svelte — Notification / status bar. Spec §8.10 -->
<script lang="ts">
  import { latestNotification, notifications } from '../stores/notifications';
  import { session, directoryCount } from '../stores/session';

  let historyOpen = $state(false);

  const TYPE_META: Record<string, { icon: string; cls: string }> = {
    idle:    { icon: '◈', cls: '' },
    info:    { icon: '◉', cls: 'status-info' },
    success: { icon: '✓', cls: 'status-success' },
    warning: { icon: '⚠', cls: 'status-warning' },
    error:   { icon: '✕', cls: 'status-error' },
    running: { icon: '◎', cls: 'status-running' },
    alert:   { icon: '✕', cls: 'status-alert' },
  };

  let meta = $derived(
    $latestNotification
      ? (TYPE_META[$latestNotification.type] ?? TYPE_META.idle)
      : TYPE_META.idle
  );

  let message = $derived($latestNotification?.message ?? 'Ready');

  function toggleHistory() {
    historyOpen = !historyOpen;
  }

  function formatTime(d: Date): string {
    return d.toLocaleTimeString('en-US', { hour12: false });
  }
</script>

<!-- Notification history overlay -->
{#if historyOpen}
  <div id="notif-history" class="open" class:running={$latestNotification?.type === 'running' || $latestNotification?.type === 'alert'}>
    <div class="notif-history-header">
      Notification History
      <span class="notif-close" onclick={() => historyOpen = false}>✕</span>
    </div>
    <div class="notif-list">
      {#each $notifications as n (n.id)}
        <div class="notif-item {n.type}">
          <span class="notif-time">{formatTime(n.time)}</span>
          <span class="notif-msg">{n.message}</span>
        </div>
      {/each}
    </div>
  </div>
{/if}

<div id="status-bar" class={meta.cls} onclick={toggleHistory}>
  <span id="status-icon">{meta.icon}</span>
  <span id="status-text">{message}</span>
  <div id="status-right">
    {#if $session.fileList.length > 0}
      <div class="status-item">
        <span class="status-item-val">
          {$session.fileList.length} file{$session.fileList.length !== 1 ? 's' : ''} · {$directoryCount} director{$directoryCount !== 1 ? 'ies' : 'y'}
        </span>
      </div>
    {/if}
  </div>
</div>
