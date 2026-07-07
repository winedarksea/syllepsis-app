// Gates PIN-lock's "remember on this device" release behind an OS biometric/device-credential
// prompt on mobile (privacy-security.md "PIN-Locked Notes"). `tauri-plugin-biometric` only has an
// Android/iOS backend (no desktop plugin registered at all — see the Cargo.toml comment on that
// dependency), so `checkStatus()` throwing or reporting unavailable means "no gate on this
// platform" and the caller should proceed straight to `unlockBookWithDeviceCredential`, matching
// desktop's story where the OS keychain prompt is itself the trust boundary.

/** Resolves `true` if it's safe to proceed with the remembered-key unlock: either the platform
 * has no biometric gate (desktop — `checkStatus` itself throws, since the plugin isn't
 * registered there), or the sensor is present and the user passed the prompt. Resolves `false`
 * only when a sensor is available and the user failed or cancelled it. */
export async function passBiometricGateIfAvailable(reason: string): Promise<boolean> {
  let authenticate: (reason: string, options?: { allowDeviceCredential?: boolean }) => Promise<void>;
  try {
    const plugin = await import('@tauri-apps/plugin-biometric');
    const status = await plugin.checkStatus();
    if (!status.isAvailable) return true;
    authenticate = plugin.authenticate;
  } catch {
    // No biometric backend on this platform (desktop) — nothing to gate on.
    return true;
  }
  try {
    await authenticate(reason, { allowDeviceCredential: true });
    return true;
  } catch {
    return false;
  }
}
