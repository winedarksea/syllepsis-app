import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
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
  unlockDelayHours?: number;
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

/** Returns the ms timestamp when an unlock_delay proposal becomes applicable. */
function unlockAtMs(item: CommentarySummary, unlockDelayHours: number): number {
  return new Date(item.created).getTime() + unlockDelayHours * 3_600_000;
}

export function CommentaryPanel({
  note,
  currentBody,
  focusId,
  onClose,
  onApplied,
  onCountChange,
  unlockDelayHours = 24,
}: Props) {
  const [items, setItems] = useState<CommentarySummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(focusId ?? null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Tick every 60 s so unlock countdowns re-evaluate without a full refresh.
  const [tick, setTick] = useState(0);
  const tickRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const lockMode = note.metadata.lifecycle?.lock ?? 'none';

  const refresh = useCallback(() => {
    api.listCommentary(note.id)
      .then((next) => {
        setItems(next);
        onCountChange?.(next.length);
      })
      .catch((err) => setError(String(err)));
  }, [note.id, onCountChange]);

  useEffect(() => { refresh(); }, [refresh]);
  useEffect(() => {
    if (focusId) setSelectedId(focusId);
  }, [focusId]);

  // Keep a 60-second ticker alive while any unlock_delay proposal is visible and pending.
  useEffect(() => {
    const hasDelayedLocked = items.some(
      (i) => i.metadata.kind === 'proposal' && i.metadata.status === 'locked' && lockMode === 'unlock_delay',
    );
    if (hasDelayedLocked) {
      if (!tickRef.current) {
        tickRef.current = setInterval(() => setTick((t) => t + 1), 60_000);
      }
    } else {
      if (tickRef.current) {
        clearInterval(tickRef.current);
        tickRef.current = null;
      }
    }
    return () => {
      if (tickRef.current) { clearInterval(tickRef.current); tickRef.current = null; }
    };
  }, [items, lockMode]);

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

  /** For a given proposal, find the FactCheck commentary that evaluates it, if any. */
  const factCheckFor = useCallback((proposalId: string) => {
    return items.find(
      (i) =>
        i.metadata.kind === 'fact_check' &&
        i.metadata.approves_commentary_id === proposalId,
    ) ?? null;
  }, [items]);

  const apply = useCallback(async (forceReplace = false) => {
    if (!selected) return;
    setBusy(true);
    setError(null);
    try {
      const updated = await api.applyCommentary(selected.id, {
        force_replace: forceReplace,
        fact_check_passed: false,
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

  /** Derive the apply-eligibility for a proposal card / detail. */
  function applyGate(item: CommentarySummary): { allowed: boolean; reason: string | null } {
    if (item.metadata.kind !== 'proposal') return { allowed: true, reason: null };

    if (lockMode === 'unlock_delay' && item.metadata.status === 'locked') {
      const unlockMs = unlockAtMs(item, unlockDelayHours);
      void tick; // depend on tick so this re-evaluates each minute
      if (Date.now() < unlockMs) {
        const unlockDate = new Date(unlockMs).toLocaleString();
        return { allowed: false, reason: `Unlocks ${unlockDate}` };
      }
      return { allowed: true, reason: null };
    }

    if (lockMode === 'fact_check_gate') {
      const fc = factCheckFor(item.id);
      if (!fc) return { allowed: false, reason: 'Pending fact-check' };
      if (fc.metadata.fact_check_passed === false) return { allowed: false, reason: 'Fact-check failed' };
      if (fc.metadata.fact_check_passed === true) return { allowed: true, reason: null };
      return { allowed: false, reason: 'Pending fact-check' };
    }

    return { allowed: true, reason: null };
  }

  /** Badge shown on the card for locked proposals. */
  function lockBadge(item: CommentarySummary): string | null {
    if (item.metadata.kind !== 'proposal') return null;

    if (lockMode === 'unlock_delay' && item.metadata.status === 'locked') {
      const unlockMs = unlockAtMs(item, unlockDelayHours);
      void tick;
      if (Date.now() < unlockMs) {
        return `Unlocks ${new Date(unlockMs).toLocaleDateString()}`;
      }
      return 'Ready to apply';
    }

    if (lockMode === 'fact_check_gate') {
      const fc = factCheckFor(item.id);
      if (!fc) return 'Fact-check: Pending';
      if (fc.metadata.fact_check_passed === true) return 'Fact-check: Passed';
      if (fc.metadata.fact_check_passed === false) return 'Fact-check: Failed';
      return 'Fact-check: Pending';
    }

    return null;
  }

  const gate = selected ? applyGate(selected) : { allowed: false, reason: null };

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
              {groups[group].map((item) => {
                const badge = lockBadge(item);
                return (
                  <button
                    key={item.id}
                    className={`commentary-card ${item.id === selectedId ? 'active' : ''}`}
                    onClick={() => setSelectedId(item.id)}
                  >
                    <span className="commentary-card-title">{item.title}</span>
                    <span className="commentary-card-meta">
                      {label(item.metadata.kind)} · {label(item.metadata.status)}
                      {badge && <span className="commentary-lock-badge">{badge}</span>}
                    </span>
                    <span className="commentary-card-body">{item.body}</span>
                  </button>
                );
              })}
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
            {gate.reason && (
              <div className="commentary-gate-notice">{gate.reason}</div>
            )}
            <div className="commentary-detail-actions">
              <button className="picker-btn picker-btn-secondary" onClick={() => setSelectedId(null)} disabled={busy}>
                Back
              </button>
              {selected.metadata.kind === 'proposal' && (
                <>
                  <button
                    className="picker-btn picker-btn-primary"
                    onClick={() => void apply(false)}
                    disabled={busy || !gate.allowed}
                    title={gate.reason ?? undefined}
                  >
                    Apply
                  </button>
                  {selected.metadata.target_field === 'body' && (
                    <button
                      className="picker-btn picker-btn-secondary"
                      onClick={() => void apply(true)}
                      disabled={busy || !gate.allowed}
                      title={gate.reason ?? undefined}
                    >
                      Replace current
                    </button>
                  )}
                </>
              )}
              {selected.metadata.status !== 'pinned' && (
                <button className="picker-btn picker-btn-secondary" onClick={pin} disabled={busy}>
                  Pin
                </button>
              )}
              <button className="picker-btn picker-btn-secondary" onClick={dismiss} disabled={busy}>
                Delete
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
}
