import { useCallback, useEffect, useRef, useState } from 'react';
import { listen } from '@tauri-apps/api/event';
import { useStore } from '../lib/store';
import { api } from '../lib/api';
import type { Category, ClassificationKind, CloudSyncFinished, ObjectType } from '../types';
import type { SignatureSlot } from '../theme/themes';
import { Icon } from './Icon';
import './Sidebar.css';

interface Props {
  onNewNote: (type?: ObjectType, classification?: ClassificationKind) => void;
  onImportImage: () => void;
  onNewDrawing: () => void;
  isMobileOpen?: boolean;
  onClose?: () => void;
}

// Object types a user can create directly from the New menu. (Commentary is produced by AI tools.)
const NEW_TYPES: { type: ObjectType; classification?: ClassificationKind; label: string }[] = [
  { type: 'note', classification: 'note', label: 'Note' },
  { type: 'note', classification: 'quote', label: 'Quote' },
  { type: 'note', classification: 'reference', label: 'Reference' },
  { type: 'note', classification: 'todo', label: 'To-do' },
  { type: 'note', classification: 'qa', label: 'Q & A' },
  { type: 'table', label: 'Table' },
  { type: 'note', classification: 'code', label: 'Code' },
];

const NAV: { view: string; icon: string; label: string; slot?: SignatureSlot }[] = [
  { view: 'book', icon: 'menu_book', label: 'Book View', slot: 'book' },
  { view: 'unsorted', icon: 'inbox', label: 'Notebox', slot: 'unsorted' },
  { view: 'search', icon: 'search', label: 'Search', slot: 'search' },
  { view: 'graph', icon: 'hub', label: 'Graph', slot: 'graph' },
  { view: 'worlds', icon: 'map', label: 'Worlds', slot: 'worlds' },
  { view: 'packs', icon: 'inventory_2', label: 'Packs', slot: 'packs' },
  { view: 'text_import', icon: 'upload_file', label: 'Note Import' },
  { view: 'privacy', icon: 'lock', label: 'Privacy' },
  { view: 'stats', icon: 'bar_chart', label: 'Statistics' },
  { view: 'style_cards', icon: 'style', label: 'Style Cards' },
  { view: 'diagnostics', icon: 'monitor_heart', label: 'Diagnostics' },
];

export function Sidebar({ onNewNote, onImportImage, onNewDrawing, isMobileOpen = false, onClose }: Props) {
  const { view, setView, categories, unsortedCount, hideUnsortedBadge, diagnosticsIssueCount, activeCategory, setActiveCategory, theme, toggleTheme, closeBook } = useStore();
  const [newMenuOpen, setNewMenuOpen] = useState(false);

  type SyncMode = 'cloud' | 'git' | 'local';
  const [syncMode, setSyncMode] = useState<SyncMode>('local');
  const [activeProvider, setActiveProvider] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [syncMsg, setSyncMsg] = useState<string | null>(null);
  const syncMsgTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const showSyncMsg = useCallback((msg: string) => {
    setSyncMsg(msg);
    if (syncMsgTimer.current) clearTimeout(syncMsgTimer.current);
    syncMsgTimer.current = setTimeout(() => setSyncMsg(null), 3000);
  }, []);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const statuses = await api.cloudSyncProviderStatuses();
        const active = statuses.find((s) => s.active_for_current_book && s.connected);
        if (!cancelled) {
          if (active) {
            setSyncMode('cloud');
            setActiveProvider(active.provider);
            return;
          }
        }
        const git = await api.gitStatus();
        if (!cancelled) {
          setSyncMode(git.is_repository ? 'git' : 'local');
        }
      } catch {
        // silently keep 'local' default
      }
    })();
    return () => { cancelled = true; };
  }, []);

  useEffect(() => {
    if (syncMode !== 'cloud' || !activeProvider) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    listen<CloudSyncFinished>('cloud-sync-finished', (event) => {
      const { provider, report, error } = event.payload;
      if (provider !== activeProvider) return;
      setSyncing(false);
      if (error) {
        showSyncMsg(`Sync error: ${error}`);
      } else if (report) {
        const total = report.pushed.length + report.pulled.length + report.merged.length;
        showSyncMsg(total > 0 ? `Synced (${total} changes)` : 'Up to date');
      }
    }).then((fn) => {
      if (disposed) { fn(); return; }
      unlisten = fn;
    }).catch(() => {});
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [syncMode, activeProvider, showSyncMsg]);

  const handleSync = useCallback(async () => {
    if (syncing) return;
    setSyncing(true);
    try {
      if (syncMode === 'cloud' && activeProvider) {
        await api.syncManagedCloudNow(activeProvider);
        // result arrives via cloud-sync-finished listener; setSyncing(false) handled there
      } else if (syncMode === 'git') {
        await api.gitPull();
        setSyncing(false);
        showSyncMsg('Git pull complete');
      } else {
        await api.operationalActivitySummary();
        setSyncing(false);
        showSyncMsg('Refreshed');
      }
    } catch (e) {
      setSyncing(false);
      showSyncMsg(String(e));
    }
  }, [syncing, syncMode, activeProvider, showSyncMsg]);

  const closeMobileDrawer = useCallback(() => {
    setNewMenuOpen(false);
    onClose?.();
  }, [onClose]);

  const handleView = useCallback((nextView: Parameters<typeof setView>[0]) => {
    setView(nextView);
    closeMobileDrawer();
  }, [closeMobileDrawer, setView]);

  const handleCategory = useCallback((cat: Category) => {
    setActiveCategory(cat.name);
    setView('category');
    closeMobileDrawer();
  }, [closeMobileDrawer, setActiveCategory, setView]);

  const createType = useCallback((type: ObjectType, classification?: ClassificationKind) => {
    setNewMenuOpen(false);
    onNewNote(type, classification);
    onClose?.();
  }, [onClose, onNewNote]);

  const handleNewNote = useCallback(() => {
    onNewNote('note', 'note');
    closeMobileDrawer();
  }, [closeMobileDrawer, onNewNote]);

  const handleImportImage = useCallback(() => {
    setNewMenuOpen(false);
    onImportImage();
    onClose?.();
  }, [onClose, onImportImage]);

  const handleNewDrawing = useCallback(() => {
    setNewMenuOpen(false);
    onNewDrawing();
    onClose?.();
  }, [onClose, onNewDrawing]);

  const handleCloseBook = useCallback(() => {
    closeBook();
    closeMobileDrawer();
  }, [closeBook, closeMobileDrawer]);

  return (
    <aside className={`sidebar ${isMobileOpen ? 'mobile-open' : ''}`} aria-label="Workspace navigation">
      <div className="sidebar-header">
        <span className="sidebar-app-name">Syllepsis</span>
        <div className="sidebar-header-actions">
          <button
            className={`sidebar-theme-btn ${view === 'settings' ? 'active' : ''}`}
            onClick={() => handleView('settings')}
            title="Settings"
          >
            <Icon name="settings" size={18} />
          </button>
          <button
            className="sidebar-theme-btn"
            onClick={handleCloseBook}
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
          <button
            className="sidebar-theme-btn sidebar-mobile-close"
            onClick={closeMobileDrawer}
            title="Close navigation"
            aria-label="Close navigation"
          >
            <Icon name="close" size={18} />
          </button>
        </div>
      </div>

      <nav className="sidebar-nav">
        {NAV.map((item) => (
          <button
            key={item.view}
            className={`sidebar-item ${view === item.view ? 'active' : ''}`}
            onClick={() => handleView(item.view as Parameters<typeof setView>[0])}
          >
            <Icon name={item.icon} slot={item.slot} className="sidebar-item-icon" size={19} />
            <span>{item.label}</span>
            {item.view === 'unsorted' && unsortedCount > 0 && !hideUnsortedBadge && (
              <span className="sidebar-badge">{unsortedCount}</span>
            )}
            {item.view === 'diagnostics' && diagnosticsIssueCount > 0 && !hideUnsortedBadge && (
              <span className="sidebar-badge sidebar-badge--diag">{diagnosticsIssueCount}</span>
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
          <button className="sidebar-new-note" onClick={handleNewNote}>
            <Icon name="add" slot="new" size={18} />
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
                <button key={`${t.type}-${t.classification ?? 'storage'}`} className="sidebar-new-menu-item" onClick={() => createType(t.type, t.classification)}>
                  {t.label}
                </button>
              ))}
              <button
                className="sidebar-new-menu-item"
                onClick={handleNewDrawing}
              >
                New Drawing
              </button>
              <button
                className="sidebar-new-menu-item"
                onClick={handleImportImage}
              >
                Import Image…
              </button>
            </div>
          )}
        </div>
        <button
          className={`sidebar-sync-btn${syncing ? ' sidebar-sync-btn--busy' : ''}`}
          onClick={handleSync}
          title={syncMsg ?? (syncing ? 'Syncing…' : syncMode === 'cloud' ? 'Sync to cloud now' : syncMode === 'git' ? 'Git pull' : 'Refresh')}
          disabled={syncing}
        >
          <Icon
            name={syncMode === 'cloud' ? 'cloud_sync' : syncMode === 'git' ? 'merge' : 'cloud_off'}
            slot="sync"
            size={18}
          />
        </button>
      </div>
    </aside>
  );
}
