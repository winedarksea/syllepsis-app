// Embedding-health diagnostics: near-duplicate notes (candidates to merge) and blind spots
// (notes weakly connected to everything else). Driven by the Rust diagnostics command.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { EmbeddingDiagnostics } from '../types';
import './Diagnostics.css';

export function Diagnostics() {
  const { openEditor, book } = useStore();
  const [diag, setDiag] = useState<EmbeddingDiagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  // Persist the last-run timestamp per book so it survives view switches.
  const lastRunKey = book ? `syllepsis.diag.lastRun.${book.path}` : null;
  const [lastRun, setLastRun] = useState<string | null>(
    () => (lastRunKey ? localStorage.getItem(lastRunKey) : null),
  );

  const run = useCallback(async () => {
    setRunning(true);
    setError(null);
    try {
      const result = await api.embeddingDiagnostics();
      setDiag(result);
      const now = new Date().toISOString();
      setLastRun(now);
      if (lastRunKey) localStorage.setItem(lastRunKey, now);
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }, [lastRunKey]);

  useEffect(() => { run(); }, [run]);

  const clean = diag && diag.duplicates.length === 0 && diag.blind_spots.length === 0;

  return (
    <div className="dg-root">
      <div className="dg-header">
        <h2 className="dg-title">Diagnostics</h2>
        <div className="dg-header-actions">
          <span className="dg-last-run">
            {lastRun ? `Last run: ${new Date(lastRun).toLocaleString()}` : 'Not run yet'}
          </span>
          <button className="dg-run-btn" onClick={run} disabled={running}>
            {running ? 'Running…' : 'Run checks'}
          </button>
        </div>
      </div>

      {error && <div className="dg-state dg-error">{error}</div>}
      {!diag && !error && <div className="dg-state">Analysing embeddings…</div>}

      {clean && (
        <div className="dg-state">No duplicates or blind spots detected. Healthy book.</div>
      )}

      {diag && diag.duplicates.length > 0 && (
        <section className="dg-section">
          <h3 className="dg-section-title">Possible duplicates ({diag.duplicates.length})</h3>
          <p className="dg-section-hint">Pairs of notes that embed very closely — consider merging.</p>
          {diag.duplicates.map((d, i) => (
            <div key={i} className="dg-pair">
              <button className="dg-link" onClick={() => openEditor(d.a_id)}>{d.a_title || '(untitled)'}</button>
              <span className="dg-sim">{Math.round(d.similarity * 100)}%</span>
              <button className="dg-link" onClick={() => openEditor(d.b_id)}>{d.b_title || '(untitled)'}</button>
            </div>
          ))}
        </section>
      )}

      {diag && diag.blind_spots.length > 0 && (
        <section className="dg-section">
          <h3 className="dg-section-title">Blind spots ({diag.blind_spots.length})</h3>
          <p className="dg-section-hint">Notes only weakly related to anything else — orphan ideas worth connecting.</p>
          {diag.blind_spots.map((b) => (
            <div key={b.note_id} className="dg-row">
              <button className="dg-link" onClick={() => openEditor(b.note_id)}>{b.title || '(untitled)'}</button>
              <span className="dg-sim dg-sim-weak">
                nearest {Math.round(b.nearest_similarity * 100)}%
              </span>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
