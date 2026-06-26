import { useEffect, useState } from 'react';
import { api } from '../lib/api';
import type { World } from '../types';
import './WorldLocationHelper.css';

interface Props {
  onApply: (token: string) => void;
}

export function WorldLocationHelper({ onApply }: Props) {
  const [open, setOpen] = useState(false);
  const [worlds, setWorlds] = useState<World[]>([]);
  const [worldId, setWorldId] = useState('');
  const [coordX, setCoordX] = useState('');
  const [coordY, setCoordY] = useState('');

  useEffect(() => {
    api.listWorlds().then(setWorlds).catch(() => {});
  }, []);

  const buildToken = (): string | null => {
    const w = worldId.trim(), x = coordX.trim(), y = coordY.trim();
    if (!x || !y) return null;
    return w ? `${w}/${x},${y}` : `${x},${y}`;
  };

  const isImage = !!worldId && worlds.find((w) => w.id === worldId)?.kind === 'image';

  return (
    <>
      <button className="wlh-toggle-btn" onClick={() => setOpen((v) => !v)}>
        {open ? 'Hide helper' : 'World helper'}
      </button>
      {open && (
        <div className="wlh-panel">
          <div className="wlh-row">
            <label className="wlh-label">World</label>
            <select value={worldId} onChange={(e) => setWorldId(e.target.value)}>
              <option value="">Default (earth)</option>
              {worlds.map((w) => <option key={w.id} value={w.id}>{w.display_name}</option>)}
            </select>
          </div>
          <div className="wlh-row">
            <label className="wlh-label">{isImage ? 'X (0–1)' : 'Latitude'}</label>
            <input value={coordX} onChange={(e) => setCoordX(e.target.value)} placeholder="0.0" />
          </div>
          <div className="wlh-row">
            <label className="wlh-label">{isImage ? 'Y (0–1)' : 'Longitude'}</label>
            <input value={coordY} onChange={(e) => setCoordY(e.target.value)} placeholder="0.0" />
          </div>
          <button
            className="wlh-apply"
            disabled={!coordX.trim() || !coordY.trim()}
            onClick={() => {
              const token = buildToken();
              if (token) { onApply(token); setOpen(false); }
            }}
          >
            Apply
          </button>
          <p className="wlh-hint">
            Result: <code>{buildToken() ?? '(enter coordinates)'}</code>
          </p>
        </div>
      )}
    </>
  );
}
