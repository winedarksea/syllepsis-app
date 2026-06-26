import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { MarkdownRenderer } from '../components/MarkdownRenderer';
import { Icon } from '../components/Icon';
import type { CommentarySummary, NoteDto } from '../types';

interface Props {
  note: NoteDto;
  currentBody: string;
  focusId?: string | null;
  onClose: () => void;
  onApplied: (note: NoteDto) => void;
  onCountChange?: (count: number) => void;
}

function label(value: string | undefined | null): string {
  return (value ?? '').replaceAll('_', ' ') || 'commentary';
}

function groupTitle(item: CommentarySummary): 'Proposals' | 'Feedback' | 'Pinned' {
  if (item.metadata.status === 'pinned' || item.metadata.kind === 'footnote') return 'Pinned';
  if (item.metadata.kind === 'proposal') return 'Proposals';
  return 'Feedback';
}

function splitLines(text: string): string[] {
  return text.length === 0 ? [''] : text.split(/\r?\n/);
}

function DiffPreview({
  base,
  current,
  proposed,
}: {
  base: string;
  current: string;
  proposed: string;
}) {
  const baseLines = splitLines(base);
  const proposedLines = splitLines(proposed);
  const max = Math.max(baseLines.length, proposedLines.length);
  return (
    <div className="commentary-diff">
      <div className="commentary-diff-column">
        <span className="commentary-diff-label">Current</span>
        <pre>{current}</pre>
      </div>
      <div className="commentary-diff-column">
        <span className="commentary-diff-label">Proposal</span>
        <pre>
          {Array.from({ length: max }).map((_, index) => {
            const before = baseLines[index] ?? '';
            const after = proposedLines[index] ?? '';
            const changed = before !== after;
            return (
              <span key={index} className={changed ? 'changed' : ''}>
                {after || ' '}
                {'\n'}
              </span>
            );
          })}
        </pre>
      </div>
    </div>
  );
}

export function CommentaryPanel({
  note,
  currentBody,
  focusId,
  onClose,
  onApplied,
  onCountChange,
}: Props) {
  const [items, setItems] = useState<CommentarySummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(focusId ?? null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(() => {
    api.listCommentary(note.id)
      .then((next) => {
        setItems(next);
        onCountChange?.(next.length);
        if (focusId) setSelectedId(focusId);
      })
      .catch((err) => setError(String(err)));
  }, [focusId, note.id, onCountChange]);

  useEffect(() => { refresh(); }, [refresh]);

  const selected = items.find((item) => item.id === selectedId) ?? null;
  const groups = useMemo(() => {
    const grouped: Record<'Proposals' | 'Feedback' | 'Pinned', CommentarySummary[]> = {
      Proposals: [],
      Feedback: [],
      Pinned: [],
    };
    for (const item of items) grouped[groupTitle(item)].push(item);
    return grouped;
  }, [items]);

  const apply = useCallback(async (forceReplace = false, factCheckPassed = false) => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const updated = await api.applyCommentary(selected.id, {
        force_replace: forceReplace,
        fact_check_passed: factCheckPassed,
      });
      onApplied(updated);
      setSelectedId(null);
      refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [onApplied, refresh, selected]);

  const dismiss = useCallback(async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await api.dismissCommentary(selected.id);
      setSelectedId(null);
      refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [refresh, selected]);

  const pin = useCallback(async () => {
    if (!selected) return;
    setBusy(true);
    try {
      await api.pinCommentary(selected.id);
      setSelectedId(null);
      refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }, [refresh, selected]);

  return (
    <>
      <aside className="commentary-panel" aria-label="Commentary">
        <div className="commentary-panel-header">
          <span>Commentary</span>
          <button onClick={onClose} title="Close commentary">
            <Icon name="close" size={16} />
          </button>
        </div>
        {error && <button className="commentary-error" onClick={() => setError(null)}>{error}</button>}
        {items.length === 0 && <div className="commentary-empty">No commentary.</div>}
        {(['Proposals', 'Feedback', 'Pinned'] as const).map((group) => (
          groups[group].length > 0 && (
            <section key={group} className="commentary-group">
              <h3>{group}</h3>
              {groups[group].map((item) => (
                <button
                  key={item.id}
                  className={`commentary-card ${item.id === selectedId ? 'active' : ''}`}
                  onClick={() => setSelectedId(item.id)}
                >
                  <span className="commentary-card-title">{item.title}</span>
                  <span className="commentary-card-meta">
                    {label(item.metadata.kind)} · {label(item.metadata.status)}
                  </span>
                  <span className="commentary-card-body">{item.body}</span>
                </button>
              ))}
            </section>
          )
        ))}
      </aside>
      {selected && (
        <div className="commentary-detail-backdrop" onClick={() => setSelectedId(null)}>
          <div className="commentary-detail" role="dialog" aria-modal="true" onClick={(event) => event.stopPropagation()}>
            <div className="commentary-detail-header">
              <div>
                <h2>{selected.title}</h2>
                <span>{label(selected.metadata.kind)} · {label(selected.metadata.source)} · {label(selected.metadata.status)}</span>
              </div>
              <button onClick={() => setSelectedId(null)} title="Close detail">
                <Icon name="close" size={18} />
              </button>
            </div>
            {selected.metadata.kind === 'proposal' && selected.metadata.target_field === 'body' ? (
              <DiffPreview
                base={selected.metadata.base_body ?? ''}
                current={currentBody}
                proposed={selected.body}
              />
            ) : (
              <MarkdownRenderer markdown={selected.body} className="commentary-detail-markdown" />
            )}
            <div className="commentary-detail-actions">
              {selected.metadata.kind === 'proposal' && (
                <>
                  <button className="picker-btn picker-btn-primary" onClick={() => apply(false)} disabled={busy}>
                    Apply
                  </button>
                  {note.metadata.lifecycle?.lock === 'fact_check_gate' && (
                    <button className="picker-btn picker-btn-secondary" onClick={() => apply(false, true)} disabled={busy}>
                      Apply with fact-check approval
                    </button>
                  )}
                  {selected.metadata.target_field === 'body' && (
                    <button className="picker-btn picker-btn-secondary" onClick={() => apply(true)} disabled={busy}>
                      Replace current
                    </button>
                  )}
                </>
              )}
              <button className="picker-btn picker-btn-secondary" onClick={pin} disabled={busy}>
                Pin
              </button>
              <button className="picker-btn picker-btn-secondary" onClick={dismiss} disabled={busy}>
                Dismiss
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
