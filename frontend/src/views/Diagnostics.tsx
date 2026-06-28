// Embedding-health diagnostics: near-duplicate notes (candidates to merge) and blind spots
// (notes weakly connected to everything else). Driven by the Rust diagnostics command.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { PageHeader } from '../components/PageHeader';
import type { EmbeddingDiagnostics } from '../types';
import './Diagnostics.css';

export function Diagnostics() {
  const { openEditor, book, setDiagnosticsIssueCount } = useStore();
  const [diag, setDiag] = useState<EmbeddingDiagnostics | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  // Persist last-run timestamp and issue count per book so both survive view switches.
  const lastRunKey = book ? `syllepsis.diag.lastRun.${book.path}` : null;
  const issueCountKey = book ? `syllepsis.diag.issueCount.${book.path}` : null;
  const [lastRun, setLastRun] = useState<string | null>(
    () => (lastRunKey ? localStorage.getItem(lastRunKey) : null),
  );

  // Seed the store badge from the persisted count on mount so it shows before first run.
  useEffect(() => {
    if (issueCountKey) {
      const stored = parseInt(localStorage.getItem(issueCountKey) ?? '0', 10);
      if (!isNaN(stored)) setDiagnosticsIssueCount(stored);
    }
  // Only run on mount (issueCountKey derived from book, won't change mid-session).
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const run = useCallback(async () => {
    setRunning(true);
    setError(null);
    try {
      const result = await api.embeddingDiagnostics();
      setDiag(result);
      const now = new Date().toISOString();
      setLastRun(now);
      if (lastRunKey) localStorage.setItem(lastRunKey, now);
      const total = result.duplicates.length + result.blind_spots.length + result.empty_notes.length;
      setDiagnosticsIssueCount(total);
      if (issueCountKey) localStorage.setItem(issueCountKey, String(total));
    } catch (e) {
      setError(String(e));
    } finally {
      setRunning(false);
    }
  }, [lastRunKey, issueCountKey, setDiagnosticsIssueCount]);

  useEffect(() => { run(); }, [run]);

  const clean = diag && diag.duplicates.length === 0 && diag.blind_spots.length === 0 && diag.empty_notes.length === 0;

  return (
    <div className="dg-root">
      <PageHeader title="Diagnostics">
        <span className="dg-last-run">
          {lastRun ? `Last run: ${new Date(lastRun).toLocaleString()}` : 'Not run yet'}
        </span>
        <button className="dg-run-btn" onClick={run} disabled={running}>
          {running ? 'Running…' : 'Run checks'}
        </button>
      </PageHeader>

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

      {diag && diag.empty_notes.length > 0 && (
        <section className="dg-section">
          <h3 className="dg-section-title">Empty notes ({diag.empty_notes.length})</h3>
          <p className="dg-section-hint">Notes with no body — excluded from search, related notes, and diagnostics until content is added.</p>
          {diag.empty_notes.map((n) => (
            <div key={n.note_id} className="dg-row">
              <button className="dg-link" onClick={() => openEditor(n.note_id)}>{n.title || '(untitled)'}</button>
            </div>
          ))}
        </section>
      )}
    </div>
  );
}
