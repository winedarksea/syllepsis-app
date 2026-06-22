import { useCallback } from 'react';
import { useStore } from '../lib/store';
import type { Category } from '../types';
import './Sidebar.css';

interface Props {
  onNewNote: () => void;
}

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
        <button className="sidebar-theme-btn" onClick={toggleTheme} title={`Switch to ${theme === 'light' ? 'dark' : 'light'} theme`}>
          {theme === 'light' ? '◑' : '●'}
        </button>
      </div>

      <nav className="sidebar-nav">
        <button
          className={`sidebar-item ${view === 'book' ? 'active' : ''}`}
          onClick={() => setView('book')}
        >
          <span className="sidebar-item-icon">📖</span>
          <span>Book View</span>
        </button>

        <button
          className={`sidebar-item ${view === 'unsorted' ? 'active' : ''}`}
          onClick={() => setView('unsorted')}
        >
          <span className="sidebar-item-icon">✦</span>
          <span>Unsorted</span>
          {unsortedCount > 0 && (
            <span className="sidebar-badge">{unsortedCount}</span>
          )}
        </button>

        <button
          className={`sidebar-item ${view === 'search' ? 'active' : ''}`}
          onClick={() => setView('search')}
        >
          <span className="sidebar-item-icon">🔍</span>
          <span>Search</span>
        </button>

        <button
          className={`sidebar-item ${view === 'graph' ? 'active' : ''}`}
          onClick={() => setView('graph')}
        >
          <span className="sidebar-item-icon">🕸</span>
          <span>Graph</span>
        </button>

        <button
          className={`sidebar-item ${view === 'worlds' ? 'active' : ''}`}
          onClick={() => setView('worlds')}
        >
          <span className="sidebar-item-icon">🗺</span>
          <span>Worlds</span>
        </button>

        <button
          className={`sidebar-item ${view === 'diagnostics' ? 'active' : ''}`}
          onClick={() => setView('diagnostics')}
        >
          <span className="sidebar-item-icon">🩺</span>
          <span>Diagnostics</span>
        </button>
      </nav>

      <div className="sidebar-section-header">Categories</div>
      <nav className="sidebar-categories">
        {categories.map((cat) => (
          <button
            key={cat.name}
            className={`sidebar-item ${view === 'category' && activeCategory === cat.name ? 'active' : ''}`}
            onClick={() => handleCategory(cat)}
          >
            <span className="sidebar-item-icon">{cat.icon ?? '#'}</span>
            <span>{cat.long_name || cat.name}</span>
          </button>
        ))}
        {categories.length === 0 && (
          <span className="sidebar-empty">No categories yet</span>
        )}
      </nav>

      <div className="sidebar-footer">
        <button className="sidebar-new-note" onClick={onNewNote}>
          + New Note
        </button>
        {/* Sync status placeholder — Phase 4 */}
        <span className="sidebar-sync-status" title="Sync: local only">◎</span>
      </div>
    </aside>
  );
}
