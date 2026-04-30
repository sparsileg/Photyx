// notifications.ts — Notification store
// Drives the status bar and notification history

import { writable, derived } from 'svelte/store';

export type NotifType = 'idle' | 'info' | 'success' | 'warning' | 'error' | 'running';

export interface Notification {
  id: number;
  message: string;
  type: NotifType;
  time: Date;
}

let nextId = 0;

function createNotificationStore() {
  const { subscribe, update } = writable<Notification[]>([]);

  function push(message: string, type: NotifType = 'info') {
    const notif: Notification = {
      id: nextId++,
      message,
      type,
      time: new Date(),
    };
    update(list => [notif, ...list].slice(0, 200));
    return notif;
  }

  return {
    subscribe,
    push,
    info:    (msg: string) => push(msg, 'info'),
    success: (msg: string) => push(msg, 'success'),
    warning: (msg: string) => push(msg, 'warning'),
    error:   (msg: string) => push(msg, 'error'),
    running: (msg: string) => push(msg, 'running'),
  };
}

export const notifications = createNotificationStore();

// Most recent notification — drives the status bar
export const latestNotification = derived(notifications, $n => $n[0] ?? null);
