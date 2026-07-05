// Auto-updater helpers around @tauri-apps/plugin-updater + plugin-process.
//
// The updater plugin is registered on desktop only (no mobile backend), so every call is
// wrapped defensively: on mobile, in the browser, or when no signing key/endpoint is
// configured, `check()` throws and we treat it as "no update available" rather than surfacing
// an error to the user.

import { check, type Update } from '@tauri-apps/plugin-updater';
import { relaunch } from '@tauri-apps/plugin-process';

export type UpdateStatus =
  | { kind: 'checking' }
  | { kind: 'available'; version: string; notes?: string; update: Update }
  | { kind: 'up-to-date' }
  | { kind: 'downloading' }
  | { kind: 'error'; message: string };

/**
 * Query the update endpoint. Returns `null` when the environment can't update (mobile,
 * dev browser, updater disabled) so callers can silently skip.
 */
export async function checkForUpdate(): Promise<Update | null> {
  try {
    return await check();
  } catch (err) {
    // Not fatal: plugin absent (mobile), offline, or no updater config.
    console.debug('update check unavailable:', err);
    return null;
  }
}

/** Download + install the given update, then relaunch into the new version. */
export async function installUpdate(update: Update): Promise<void> {
  await update.downloadAndInstall();
  await relaunch();
}

/**
 * Silent startup check: resolves to an available update (for a non-blocking prompt) or null.
 * Never throws.
 */
export async function checkForUpdateOnStartup(): Promise<Update | null> {
  const update = await checkForUpdate();
  return update && update.available ? update : null;
}
