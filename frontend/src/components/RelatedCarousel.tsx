// A horizontal strip of notes related to a focused note, by embedding similarity
// (category-upweighted on the Rust side). Reused in the editor and the search view.

import { useEffect, useState } from 'react';
import { api } from '../lib/api';
import { displayTitle } from '../lib/utils';
import { useStore } from '../lib/store';
import type { RelatedNote } from '../types';
import './RelatedCarousel.css';

interface Props {
  noteId: string;
}

export function RelatedCarousel({ noteId }: Props) {
  const { openEditor } = useStore();
  const [related, setRelated] = useState<RelatedNote[]>([]);
  const [loadedForNoteId, setLoadedForNoteId] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(() =>
    typeof window !== 'undefined' && window.matchMedia('(max-width: 720px)').matches,
  );

  const loading = loadedForNoteId !== noteId;

  useEffect(() => {
    let cancelled = false;
    api.relatedNotes(noteId)
      .then((r) => {
        if (!cancelled) {
          setRelated(r);
          setLoadedForNoteId(noteId);
        }
      })
      .catch(() => {
        if (!cancelled) {
          setRelated([]);
          setLoadedForNoteId(noteId);
        }
      });
    return () => { cancelled = true; };
  }, [noteId]);

  if (loading) return <div className="rc-state">Finding related notes…</div>;
  if (related.length === 0) return null;

  return (
    <div className="rc-root">
      <button className="rc-label rc-toggle" onClick={() => setCollapsed((value) => !value)}>
        Related notes {collapsed ? `(${related.length})` : ''}
      </button>
      {!collapsed && (
        <div className="rc-track">
          {related.map((r) => (
            <button
              key={r.note_id}
              className="rc-card"
              onClick={() => openEditor(r.note_id)}
              title={`${Math.round(r.similarity * 100)}% similar`}
            >
              <div className="rc-card-title">{displayTitle(r.title, r.summary)}</div>
              {r.summary && <div className="rc-card-summary">{r.summary}</div>}
              <div className="rc-card-foot">
                <span className="rc-sim">{Math.round(r.similarity * 100)}%</span>
                {r.shares_category && <span className="rc-shared" title="Shares a category">#</span>}
              </div>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
