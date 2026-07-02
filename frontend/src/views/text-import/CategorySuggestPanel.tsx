import { useCallback, useState } from 'react';
import { api } from '../../lib/api';
import { Icon } from '../../components/Icon';
import type { TextImportCategorySuggestion, TextImportPreviewItem } from '../../types';

interface CategorySuggestPanelProps {
  items: TextImportPreviewItem[];
  existingCategories: string[];
  /** Add `name` to the `categories` of the items at these positions. */
  onApply: (name: string, itemPositions: number[]) => void;
  onError: (message: string) => void;
}

/**
 * Deterministic keyword category suggestions over the current preview. Suggestions are computed
 * on demand (the preview can be large) and applied in bulk to every matching item.
 */
export function CategorySuggestPanel({ items, existingCategories, onApply, onError }: CategorySuggestPanelProps) {
  const [suggestions, setSuggestions] = useState<TextImportCategorySuggestion[] | null>(null);
  const [dismissed, setDismissed] = useState<Set<string>>(new Set());
  const [applied, setApplied] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState(false);

  const run = useCallback(async () => {
    setBusy(true);
    try {
      setSuggestions(await api.suggestTextImportCategories(items, 12));
      setDismissed(new Set());
      setApplied(new Set());
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [items, onError]);

  const visible = (suggestions ?? []).filter((s) => !dismissed.has(s.name));

  return (
    <section className="ti-subpanel">
      <div className="ti-subpanel-header">
        <h4>Category suggestions</h4>
        <button className="ti-btn" disabled={busy || items.length === 0} onClick={run}>
          <Icon name="label" size={17} />
          <span>{suggestions ? 'Refresh' : 'Suggest categories'}</span>
        </button>
      </div>

      {suggestions !== null && visible.length === 0 && (
        <p className="ti-subtitle">No recurring terms stood out. Try after editing titles, or add categories per note.</p>
      )}

      {visible.length > 0 && (
        <ul className="ti-suggestion-list">
          {visible.map((suggestion) => {
            const exists = existingCategories.includes(suggestion.name);
            const wasApplied = applied.has(suggestion.name);
            return (
              <li key={suggestion.name} className="ti-suggestion">
                <span className="ti-chip ti-chip-primary">#{suggestion.name}</span>
                <span className="ti-suggestion-meta">
                  {suggestion.item_indices.length} note{suggestion.item_indices.length === 1 ? '' : 's'}
                  {' · '}score {suggestion.score.toFixed(1)}
                  {exists && <span className="ti-suggestion-exists"> · already exists</span>}
                </span>
                <button
                  className="ti-btn ti-btn-small"
                  disabled={wasApplied}
                  onClick={() => {
                    onApply(suggestion.name, suggestion.item_indices);
                    setApplied((current) => new Set(current).add(suggestion.name));
                  }}
                >
                  {wasApplied ? 'Applied' : `Apply to ${suggestion.item_indices.length}`}
                </button>
                <button
                  className="ti-icon-btn"
                  onClick={() => setDismissed((current) => new Set(current).add(suggestion.name))}
                  title="Dismiss suggestion"
                >
                  <Icon name="close" size={15} />
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}
