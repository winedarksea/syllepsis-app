// PIN-locked notes (privacy-security.md "PIN-Locked Notes"): session status + the shared
// "ask the user to unlock" flow. A single Zustand store (matching the rest of the app's global
// state) rather than React context — `requestUnlock()` is promise-based so any call site (editor,
// packs export, settings) can `await` an unlock without owning modal state itself.

import { create } from 'zustand';
import { listen } from '@tauri-apps/api/event';
import { api } from './api';
import type { PinLockStatus } from '../types';

export const PIN_SESSION_EVENT = 'pin-session-changed';

interface PinLockState {
  status: PinLockStatus | null;
  modalOpen: boolean;
  refresh: () => Promise<PinLockStatus | null>;
  requestUnlock: () => Promise<boolean>;
  resolveModal: (unlocked: boolean) => void;
}

let pendingResolvers: Array<(unlocked: boolean) => void> = [];

export const usePinLockStore = create<PinLockState>((set) => ({
  status: null,
  modalOpen: false,
  refresh: async () => {
    try {
      const status = await api.getPinLockStatus();
      set({ status });
      return status;
    } catch {
      // No book open, or the command isn't reachable yet — leave status as-is.
      return null;
    }
  },
  requestUnlock: () => {
    set({ modalOpen: true });
    return new Promise<boolean>((resolve) => {
      pendingResolvers.push(resolve);
    });
  },
  resolveModal: (unlocked) => {
    set({ modalOpen: false });
    const resolvers = pendingResolvers;
    pendingResolvers = [];
    for (const resolve of resolvers) resolve(unlocked);
  },
}));

/** True for the two error strings the backend uses to signal "an unlock is required". */
export function isPinRequiredError(error: unknown): boolean {
  const message = String(error);
  return message.includes('pin_required') || message.includes('book is locked');
}

let listenerStarted = false;

/** Start the `pin-session-changed` listener once (idle auto-relock, unlocks/locks from any
 * command). Safe to call from multiple components — only the first call attaches the listener. */
export function startPinLockListener() {
  if (listenerStarted) return;
  listenerStarted = true;
  listen(PIN_SESSION_EVENT, () => {
    usePinLockStore.getState().refresh();
  }).catch(() => {});
}
