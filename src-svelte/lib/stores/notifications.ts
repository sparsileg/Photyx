// notifications.ts — Notification store
// Drives the status bar and notification history

import { writable, derived } from 'svelte/store';

export type NotifType = 'idle' | 'info' | 'success' | 'warning' | 'error' | 'running' | 'alert';

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
    alert:   (alertMsg: string, errorMsg: string, durationMs: number = 10000) => {
      const notif = push(alertMsg, 'alert');
      setTimeout(() => {
        update(list => {
          if (list.length > 0 && list[0].id === notif.id) {
            const updated = { ...list[0], type: 'error' as NotifType, message: errorMsg };
            return [updated, ...list.slice(1)];
          }
          return list;
        });
      }, durationMs);
    },
  };
}

export const notifications = createNotificationStore();

// Most recent notification — drives the status bar
export const latestNotification = derived(notifications, $n => $n[0] ?? null);
