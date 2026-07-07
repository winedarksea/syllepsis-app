// Shared unlock prompt for PIN-locked notes (privacy-security.md "PIN-Locked Notes"). Rendered
// once near the app root; opened via `usePinLockStore().requestUnlock()` from any call site
// (editor, packs export, settings) that hit a `pin_required`/"book is locked" error.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { usePinLockStore } from '../lib/pinLock';
import { passBiometricGateIfAvailable } from '../lib/biometric';
import './UnlockPinModal.css';

export function UnlockPinModal() {
  const modalOpen = usePinLockStore((s) => s.modalOpen);
  const status = usePinLockStore((s) => s.status);
  const resolveModal = usePinLockStore((s) => s.resolveModal);
  const refresh = usePinLockStore((s) => s.refresh);

  const [pin, setPin] = useState('');
  const [remember, setRemember] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!modalOpen) {
      setPin('');
      setRemember(false);
      setError(null);
      setBusy(false);
    }
  }, [modalOpen]);

  const submit = useCallback(async (e: React.FormEvent) => {
    e.preventDefault();
    if (!pin) return;
    setBusy(true);
    setError(null);
    try {
      await api.unlockBook(pin, remember);
      await refresh();
      resolveModal(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [pin, remember, refresh, resolveModal]);

  const useDeviceUnlock = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const passed = await passBiometricGateIfAvailable('Unlock this book');
      if (!passed) { setError('Biometric authentication failed.'); return; }
      await api.unlockBookWithDeviceCredential();
      await refresh();
      resolveModal(true);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [refresh, resolveModal]);

  const cancel = useCallback(() => resolveModal(false), [resolveModal]);

  if (!modalOpen) return null;

  return (
    <div className="upm-backdrop" role="presentation">
      <form className="upm-panel" role="dialog" aria-modal="true" aria-labelledby="upm-title" onSubmit={submit}>
        <h2 id="upm-title">Enter PIN</h2>
        <p className="upm-hint">This book is PIN-locked. Enter your PIN to view or edit locked notes.</p>
        {status?.hint && <p className="upm-book-hint">Hint: {status.hint}</p>}

        <input
          type="password"
          inputMode="numeric"
          autoFocus
          value={pin}
          onChange={(e) => setPin(e.target.value)}
          placeholder="PIN"
          className="upm-input"
        />

        <label className="upm-remember">
          <input type="checkbox" checked={remember} onChange={(e) => setRemember(e.target.checked)} />
          <span>Remember on this device</span>
        </label>

        {error && <div className="upm-error">{error}</div>}

        <div className="upm-actions">
          <button type="button" className="upm-btn" onClick={cancel} disabled={busy}>Cancel</button>
          {status?.remembered_key_available && (
            <button type="button" className="upm-btn" onClick={useDeviceUnlock} disabled={busy}>
              Use device unlock
            </button>
          )}
          <button type="submit" className="upm-btn upm-btn-primary" disabled={busy || !pin}>
            Unlock
          </button>
        </div>
      </form>
    </div>
  );
}
