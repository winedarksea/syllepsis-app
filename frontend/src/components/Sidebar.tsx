import { useCallback, useState } from 'react';
import { useStore } from '../lib/store';
import type { Category, ObjectType } from '../types';
import { Icon } from './Icon';
import './Sidebar.css';

interface Props {
  onNewNote: (type?: ObjectType) => void;
}

// Object types a user can create directly from the New menu. (Picture/Drawing need asset
// authoring that isn't built yet; Commentary is produced by the AI tools, not created by hand.)
const NEW_TYPES: { type: ObjectType; label: string }[] = [
  { type: 'note', label: 'Note' },
  { type: 'quote', label: 'Quote' },
  { type: 'reference', label: 'Reference' },
  { type: 'todo', label: 'To-do' },
  { type: 'qa', label: 'Q & A' },
  { type: 'table', label: 'Table' },
  { type: 'code', label: 'Code' },
];

const NAV: { view: string; icon: string; label: string }[] = [
  { view: 'book', icon: 'menu_book', label: 'Book View' },
  { view: 'unsorted', icon: 'inbox', label: 'Unsorted' },
  { view: 'search', icon: 'search', label: 'Search' },
  { view: 'graph', icon: 'hub', label: 'Graph' },
  { view: 'worlds', icon: 'map', label: 'Worlds' },
  { view: 'packs', icon: 'inventory_2', label: 'Packs' },
  { view: 'privacy', icon: 'lock', label: 'Privacy' },
  { view: 'diagnostics', icon: 'monitor_heart', label: 'Diagnostics' },
];

export function Sidebar({ onNewNote }: Props) {
  const { view, setView, categories, unsortedCount, activeCategory, setActiveCategory, theme, toggleTheme, closeBook } = useStore();
  const [newMenuOpen, setNewMenuOpen] = useState(false);

  const handleCategory = useCallback((cat: Category) => {
    setActiveCategory(cat.name);
    setView('category');
  }, [setActiveCategory, setView]);

  const createType = useCallback((type: ObjectType) => {
    setNewMenuOpen(false);
    onNewNote(type);
  }, [onNewNote]);

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-app-name">Syllepsis</span>
        <div className="sidebar-header-actions">
          <button
            className="sidebar-theme-btn"
            onClick={closeBook}
            title="Close book — back to launch screen"
          >
            <Icon name="logout" size={18} />
          </button>
          <button
            className="sidebar-theme-btn"
            onClick={toggleTheme}
            title={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}
          >
            <Icon name={theme === 'light' ? 'dark_mode' : 'light_mode'} size={18} />
          </button>
        </div>
      </div>

      <nav className="sidebar-nav">
        {NAV.map((item) => (
          <button
            key={item.view}
            className={`sidebar-item ${view === item.view ? 'active' : ''}`}
            onClick={() => setView(item.view as Parameters<typeof setView>[0])}
          >
            <Icon name={item.icon} className="sidebar-item-icon" size={19} />
            <span>{item.label}</span>
            {item.view === 'unsorted' && unsortedCount > 0 && (
              <span className="sidebar-badge">{unsortedCount}</span>
            )}
          </button>
        ))}
      </nav>

      <div className="sidebar-section-header">Categories</div>
      <nav className="sidebar-categories">
        {categories.map((cat) => (
          <button
            key={cat.name}
            className={`sidebar-item ${view === 'category' && activeCategory === cat.name ? 'active' : ''}`}
            onClick={() => handleCategory(cat)}
          >
            {cat.icon
              ? <span className="sidebar-item-icon sidebar-item-emoji">{cat.icon}</span>
              : <Icon name="tag" className="sidebar-item-icon" size={19} />}
            <span>{cat.long_name || cat.name}</span>
          </button>
        ))}
        {categories.length === 0 && (
          <span className="sidebar-empty">No categories yet</span>
        )}
      </nav>

      <div className="sidebar-footer">
        <div className="sidebar-new-group">
          <button className="sidebar-new-note" onClick={() => onNewNote('note')}>
            <Icon name="add" size={18} />
            <span>New Note</span>
          </button>
          <button
            className="sidebar-new-caret"
            onClick={() => setNewMenuOpen((v) => !v)}
            title="Create another type"
            aria-label="Create another type"
          >
            <Icon name="expand_more" size={18} />
          </button>
          {newMenuOpen && (
            <div className="sidebar-new-menu">
              {NEW_TYPES.map((t) => (
                <button key={t.type} className="sidebar-new-menu-item" onClick={() => createType(t.type)}>
                  {t.label}
                </button>
              ))}
            </div>
          )}
        </div>
        {/* Sync status placeholder — Phase 4 */}
        <Icon name="cloud_off" className="sidebar-sync-status" size={18} title="Sync: local only" />
      </div>
    </aside>
  );
}
