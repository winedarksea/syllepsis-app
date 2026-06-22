import { useCallback } from 'react';
import { useStore } from '../lib/store';
import type { Category } from '../types';
import { Icon } from './Icon';
import './Sidebar.css';

interface Props {
  onNewNote: () => void;
}

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
  const { view, setView, categories, unsortedCount, activeCategory, setActiveCategory, theme, toggleTheme } = useStore();

  const handleCategory = useCallback((cat: Category) => {
    setActiveCategory(cat.name);
    setView('category');
  }, [setActiveCategory, setView]);

  return (
    <aside className="sidebar">
      <div className="sidebar-header">
        <span className="sidebar-app-name">Syllepsis</span>
        <button
          className="sidebar-theme-btn"
          onClick={toggleTheme}
          title={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}
        >
          <Icon name={theme === 'light' ? 'dark_mode' : 'light_mode'} size={18} />
        </button>
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
        <button className="sidebar-new-note" onClick={onNewNote}>
          <Icon name="add" size={18} />
          <span>New Note</span>
        </button>
        {/* Sync status placeholder — Phase 4 */}
        <Icon name="cloud_off" className="sidebar-sync-status" size={18} title="Sync: local only" />
      </div>
    </aside>
  );
}
