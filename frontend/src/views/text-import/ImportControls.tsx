import type { Category, NoteDto, PluginDescriptor, TextImportOptions, TextImportSplitMode } from '../../types';
import { Icon } from '../../components/Icon';

export const TEXT_SOURCE = 'text';

const SPLIT_MODES: { value: TextImportSplitMode; label: string }[] = [
  { value: 'smart', label: 'Smart blocks' },
  { value: 'outline', label: 'Outline (indented)' },
  { value: 'paragraph', label: 'Paragraphs' },
  { value: 'non_empty_line', label: 'Lines' },
  { value: 'one_note', label: 'One note' },
];

export type PlacementMode = 'unsorted' | 'category' | 'after_note';

interface ImportControlsProps {
  source: string;
  importPlugins: PluginDescriptor[];
  activePlugin: PluginDescriptor | null;
  onSourceChange: (source: string) => void;
  sourceText: string;
  onSourceTextChange: (text: string) => void;
  onChooseFile: () => void;
  onChoosePluginFile: () => void;
  options: TextImportOptions;
  onOptionChange: <K extends keyof TextImportOptions>(key: K, value: TextImportOptions[K]) => void;
  placementMode: PlacementMode;
  onPlacementModeChange: (mode: PlacementMode) => void;
  placementCategory: string;
  onPlacementCategoryChange: (category: string) => void;
  placementNoteId: string;
  onPlacementNoteIdChange: (noteId: string) => void;
  categories: Category[];
  notes: NoteDto[];
  busy: boolean;
  onPreview: () => void;
  commitDisabled: boolean;
  commitCount: number;
  onCommit: () => void;
}

export function ImportControls(props: ImportControlsProps) {
  const {
    source, importPlugins, activePlugin, onSourceChange,
    sourceText, onSourceTextChange, onChooseFile, onChoosePluginFile,
    options, onOptionChange,
    placementMode, onPlacementModeChange,
    placementCategory, onPlacementCategoryChange,
    placementNoteId, onPlacementNoteIdChange,
    categories, notes, busy, onPreview, commitDisabled, commitCount, onCommit,
  } = props;

  return (
    <>
      <label className="ti-field">
        <span>Source</span>
        <div className="ti-field-row">
          <select value={source} onChange={(event) => onSourceChange(event.target.value)}>
            <option value={TEXT_SOURCE}>Paste or text file</option>
            {importPlugins.map((plugin) => (
              <option key={plugin.id} value={plugin.id}>
                {plugin.name}{plugin.import_extensions.length > 0 ? ` (.${plugin.import_extensions.join(', .')})` : ''}
              </option>
            ))}
          </select>
          {source === TEXT_SOURCE ? (
            <button className="ti-btn" disabled={busy} onClick={onChooseFile}>
              <Icon name="folder_open" size={18} />
              <span>Choose file</span>
            </button>
          ) : (
            <button className="ti-btn ti-btn-primary" disabled={busy} onClick={onChoosePluginFile}>
              <Icon name="folder_open" size={18} />
              <span>Choose {activePlugin?.name ?? 'file'}</span>
            </button>
          )}
        </div>
      </label>

      {source === TEXT_SOURCE ? (
        <label className="ti-field">
          <span>Source text</span>
          <textarea
            className="ti-source"
            value={sourceText}
            onChange={(event) => onSourceTextChange(event.target.value)}
            placeholder="Paste plain text, Markdown, or a tab-indented outline here."
          />
        </label>
      ) : (
        <p className="ti-subtitle">
          {activePlugin?.description ?? 'Choose a file above to extract its text, then preview and chunk it into notes.'}
        </p>
      )}

      <div className="ti-controls">
        <label className="ti-field">
          <span>Split mode</span>
          <select value={options.split_mode} onChange={(event) => onOptionChange('split_mode', event.target.value as TextImportSplitMode)}>
            {SPLIT_MODES.map((mode) => (
              <option key={mode.value} value={mode.value}>{mode.label}</option>
            ))}
          </select>
        </label>

        <label className="ti-field">
          <span>Placement</span>
          <select value={placementMode} onChange={(event) => onPlacementModeChange(event.target.value as PlacementMode)}>
            <option value="unsorted">Unsorted or detected sections</option>
            <option value="category">Start in category</option>
            <option value="after_note">After existing note</option>
          </select>
        </label>

        {placementMode === 'category' && (
          <label className="ti-field">
            <span>Category</span>
            <select value={placementCategory} onChange={(event) => onPlacementCategoryChange(event.target.value)}>
              <option value="">Choose category</option>
              {categories.map((category) => (
                <option key={category.name} value={category.name}>#{category.name}</option>
              ))}
            </select>
          </label>
        )}

        {placementMode === 'after_note' && (
          <label className="ti-field">
            <span>Previous note</span>
            <select value={placementNoteId} onChange={(event) => onPlacementNoteIdChange(event.target.value)}>
              <option value="">Choose note</option>
              {notes.map((note) => (
                <option key={note.id} value={note.id}>{note.title || note.id}</option>
              ))}
            </select>
          </label>
        )}
      </div>

      {options.split_mode === 'outline' ? (
        <div className="ti-outline-options">
          <div className="ti-controls">
            <NumberField
              label="Promote at lines"
              hint="Subtrees with at least this many lines become their own note."
              value={options.outline_promote_min_lines}
              min={2}
              onChange={(value) => onOptionChange('outline_promote_min_lines', value)}
            />
            <NumberField
              label="Promote at characters"
              hint="…or at least this many characters."
              value={options.outline_promote_min_chars}
              min={50}
              step={50}
              onChange={(value) => onOptionChange('outline_promote_min_chars', value)}
            />
            <NumberField
              label="Max promotion depth"
              hint="0 disables subtree promotion."
              value={options.outline_promote_max_depth}
              min={0}
              onChange={(value) => onOptionChange('outline_promote_max_depth', value)}
            />
            <NumberField
              label="Section min children"
              hint="Top-level lines with this many children become categories."
              value={options.outline_category_min_children}
              min={1}
              onChange={(value) => onOptionChange('outline_category_min_children', value)}
            />
          </div>
          <div className="ti-toggles">
            <Toggle
              label="Group loose top-level lines"
              checked={options.outline_group_loose_lines}
              onChange={(value) => onOptionChange('outline_group_loose_lines', value)}
            />
          </div>
        </div>
      ) : (
        <div className="ti-toggles">
          <Toggle label="Headings to sections" checked={options.detect_headings} onChange={(value) => onOptionChange('detect_headings', value)} />
          <Toggle label="Detect lists" checked={options.detect_lists} onChange={(value) => onOptionChange('detect_lists', value)} />
          <Toggle label="Detect tables" checked={options.detect_tables} onChange={(value) => onOptionChange('detect_tables', value)} />
          <Toggle label="Detect code fences" checked={options.detect_code_blocks} onChange={(value) => onOptionChange('detect_code_blocks', value)} />
          <Toggle label="Indented lines as bullets" checked={options.convert_indented_lists} onChange={(value) => onOptionChange('convert_indented_lists', value)} />
        </div>
      )}

      <div className="ti-actions">
        {source === TEXT_SOURCE ? (
          <button className="ti-btn ti-btn-primary" disabled={busy || !sourceText.trim()} onClick={onPreview}>
            <Icon name="visibility" size={18} />
            <span>Preview import</span>
          </button>
        ) : (
          <button className="ti-btn" disabled={busy} onClick={onChoosePluginFile}>
            <Icon name="visibility" size={18} />
            <span>Re-run preview</span>
          </button>
        )}
        <button className="ti-btn ti-btn-primary" disabled={commitDisabled} onClick={onCommit}>
          <Icon name="upload_file" size={18} />
          <span>Import {commitCount || ''} note{commitCount === 1 ? '' : 's'}</span>
        </button>
      </div>
    </>
  );
}

function NumberField({ label, hint, value, min, step, onChange }: {
  label: string;
  hint?: string;
  value: number;
  min?: number;
  step?: number;
  onChange: (value: number) => void;
}) {
  return (
    <label className="ti-field" title={hint}>
      <span>{label}</span>
      <input
        type="number"
        value={value}
        min={min}
        step={step ?? 1}
        onChange={(event) => {
          const next = Number(event.target.value);
          if (Number.isFinite(next)) onChange(Math.max(min ?? 0, Math.floor(next)));
        }}
      />
    </label>
  );
}

export function Toggle({ label, checked, onChange }: { label: string; checked: boolean; onChange: (value: boolean) => void }) {
  return (
    <label className="ti-toggle">
      <input type="checkbox" checked={checked} onChange={(event) => onChange(event.target.checked)} />
      <span>{label}</span>
    </label>
  );
}
