import { useState } from 'react';
import type { TextImportPreviewItem } from '../../types';
import { Icon } from '../../components/Icon';

const BLOCK_LABEL: Record<string, string> = {
  paragraph: 'Paragraph',
  list: 'List',
  table: 'Table',
  code: 'Code',
};

export interface EditableImportItem extends TextImportPreviewItem {
  accepted: boolean;
}

interface PreviewItemCardProps {
  item: EditableImportItem;
  displayNumber: number;
  /** Compact cards render read-only until expanded (large-preview windowing). */
  compact: boolean;
  onChange: (patch: Partial<EditableImportItem>) => void;
  onRemove: () => void;
}

/**
 * One preview note card. The category add-input references the shared
 * `#ti-known-categories` datalist rendered once by the view.
 */
export function PreviewItemCard({ item, displayNumber, compact, onChange, onRemove }: PreviewItemCardProps) {
  const [expanded, setExpanded] = useState(false);
  const [newCategory, setNewCategory] = useState('');
  const editable = !compact || expanded;

  const addCategory = () => {
    const slug = newCategory.trim().toLowerCase().replace(/^#/, '').replace(/\s+/g, '-');
    if (!slug) return;
    if (!item.categories.includes(slug) && item.category_context !== slug) {
      onChange({ categories: [...item.categories, slug] });
    }
    setNewCategory('');
  };

  return (
    <article
      className={`ti-preview-item${item.accepted ? '' : ' ti-item-rejected'}`}
      style={item.depth > 0 ? { marginLeft: `${Math.min(item.depth, 4) * 20}px` } : undefined}
    >
      <div className="ti-preview-item-header">
        <span className="ti-item-index">{displayNumber}</span>
        <span className={`ti-kind ti-kind-${item.block_kind}`}>{BLOCK_LABEL[item.block_kind]}</span>
        {item.parent_index != null && (
          <span className="ti-chip ti-chip-split" title="Promoted out of a larger outline subtree">
            <Icon name="call_split" size={14} />
            <span>split from parent</span>
          </span>
        )}
        {item.intended_prior && <span className="ti-prior">{item.intended_prior.kind}</span>}
        <span className="ti-item-header-spacer" />
        <button
          className={`ti-icon-btn ti-accept-btn${item.accepted ? ' ti-accept-btn-on' : ''}`}
          onClick={() => onChange({ accepted: !item.accepted })}
          title={item.accepted ? 'Excluded from import when toggled off' : 'Rejected — click to include again'}
        >
          <Icon name={item.accepted ? 'check_circle' : 'radio_button_unchecked'} size={17} />
        </button>
        {compact && (
          <button className="ti-icon-btn" onClick={() => setExpanded((value) => !value)} title={expanded ? 'Collapse' : 'Edit'}>
            <Icon name={expanded ? 'unfold_less' : 'edit'} size={17} />
          </button>
        )}
        <button className="ti-icon-btn" onClick={onRemove} title="Remove from import">
          <Icon name="close" size={17} />
        </button>
      </div>

      {editable ? (
        <>
          <input
            className="ti-title-input"
            value={item.title}
            onChange={(event) => onChange({ title: event.target.value })}
          />
          <textarea
            className="ti-body-input"
            value={item.body}
            onChange={(event) => onChange({ body: event.target.value })}
          />
        </>
      ) : (
        <div className="ti-compact-preview">
          <strong>{item.title || 'Untitled'}</strong>
          <span>{firstLine(item.body)}</span>
        </div>
      )}

      <div className="ti-category-editor">
        {item.category_context && (
          <span className="ti-chip ti-chip-primary" title="Section category from the split">
            #{item.category_context}
          </span>
        )}
        {item.categories.map((category) => (
          <span key={category} className="ti-chip">
            #{category}
            <button
              className="ti-chip-remove"
              onClick={() => onChange({ categories: item.categories.filter((c) => c !== category) })}
              title={`Remove #${category}`}
            >
              <Icon name="close" size={13} />
            </button>
          </span>
        ))}
        {editable && (
          <span className="ti-chip-add">
            <input
              list="ti-known-categories"
              value={newCategory}
              placeholder="+ category"
              onChange={(event) => setNewCategory(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === 'Enter') {
                  event.preventDefault();
                  addCategory();
                }
              }}
              onBlur={addCategory}
            />
          </span>
        )}
      </div>

      {item.warnings.map((warning) => (
        <div key={warning} className="ti-warning">{warning}</div>
      ))}
    </article>
  );
}

function firstLine(text: string): string {
  const line = text.split('\n', 1)[0] ?? '';
  return line.length > 140 ? `${line.slice(0, 140)}…` : line;
}
