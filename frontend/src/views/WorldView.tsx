import { useCallback, useEffect, useMemo, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { Icon } from '../components/Icon';
import type { NoteDto, Overlay, World } from '../types';
import { WorldStage } from './WorldStage';
import './WorldView.css';

const IMAGE_FILTER = [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'svg'] }];

export function WorldView() {
  const {
    activeWorld, setActiveWorld, openEditor, setActiveCategory, setView, book,
  } = useStore();
  const [worlds, setWorlds] = useState<World[]>([]);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const [backdrop, setBackdrop] = useState<string | null>(null);
  const [showCreate, setShowCreate] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reloadWorlds = useCallback(async (preferredWorldId?: string) => {
    const loadedWorlds = await api.listWorlds();
    setWorlds(loadedWorlds);
    const nextWorld = preferredWorldId
      ?? activeWorld
      ?? loadedWorlds.find((world) => world.kind === 'image')?.id
      ?? loadedWorlds[0]?.id
      ?? null;
    if (nextWorld) setActiveWorld(nextWorld);
  }, [activeWorld, setActiveWorld]);

  useEffect(() => {
    // Initial asynchronous load for the selected book.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    reloadWorlds().catch((caught) => setError(String(caught)));
  }, [reloadWorlds]);

  useEffect(() => {
    if (!activeWorld) return;
    // Clear stale geometry before the new world arrives.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setOverlay(null);
    Promise.all([api.worldOverlay(activeWorld), api.worldBackdrop(activeWorld)])
      .then(([loadedOverlay, loadedBackdrop]) => {
        setOverlay(loadedOverlay);
        setBackdrop(loadedBackdrop);
        setError(null);
      })
      .catch((caught) => setError(String(caught)));
  }, [activeWorld]);

  const selectedWorld = worlds.find((world) => world.id === activeWorld);
  const gridStorageKey = book && activeWorld
    ? `syllepsis.worldGrid.${book.path}.${activeWorld}`
    : null;
  const [showGrid, setShowGrid] = useState(false);
  useEffect(() => {
    // Grid preference is external persisted state keyed by book/world.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setShowGrid(gridStorageKey ? localStorage.getItem(gridStorageKey) === 'true' : false);
  }, [gridStorageKey]);

  const toggleGrid = () => {
    const next = !showGrid;
    setShowGrid(next);
    if (gridStorageKey) localStorage.setItem(gridStorageKey, String(next));
  };

  const openCategory = (name: string) => {
    setActiveCategory(name);
    setView('category');
  };

  const deleteSelectedWorld = async () => {
    if (!selectedWorld || selectedWorld.id === 'earth') return;
    try {
      const impact = await api.worldDeletionImpact(selectedWorld.id);
      const references = impact.note_references + impact.category_references + impact.lookup_references;
      if (references > 0) {
        setError(`Cannot delete ${selectedWorld.display_name}: ${references} saved location reference(s) still use it.`);
        return;
      }
      if (!window.confirm(`Delete world “${selectedWorld.display_name}”? The image object will be kept.`)) return;
      await api.deleteWorld(selectedWorld.id);
      setActiveWorld('earth');
      await reloadWorlds('earth');
    } catch (caught) {
      setError(String(caught));
    }
  };

  return (
    <div className="wv-root">
      <header className="wv-header">
        <div className="wv-heading-row">
          <h2 className="wv-title">Worlds</h2>
          <div className="wv-header-actions">
            <button onClick={toggleGrid} className={showGrid ? 'active' : ''}>
              <Icon name="grid_on" size={17} /> Grid
            </button>
            {selectedWorld?.id !== 'earth' && (
              <button onClick={deleteSelectedWorld} title="Delete selected world">
                <Icon name="delete" size={17} />
              </button>
            )}
            <button className="wv-create-button" onClick={() => setShowCreate(true)}>
              <Icon name="add" size={17} /> New world
            </button>
          </div>
        </div>
        <div className="wv-world-tabs">
          {worlds.map((world) => (
            <button
              key={world.id}
              className={`wv-world-tab ${activeWorld === world.id ? 'active' : ''}`}
              onClick={() => setActiveWorld(world.id)}
            >
              <Icon name={world.kind === 'image' ? 'map' : 'public'} size={16} />
              {world.display_name}
            </button>
          ))}
        </div>
      </header>

      {error && <div className="wv-error-banner" onClick={() => setError(null)}>{error}</div>}
      {overlay ? (
        <WorldStage
          overlay={overlay}
          backdrop={backdrop}
          showGrid={showGrid}
          onOpenNote={openEditor}
          onOpenCategory={openCategory}
        />
      ) : (
        <div className="wv-state">Loading world…</div>
      )}

      {showCreate && (
        <CreateWorldDialog
          onCancel={() => setShowCreate(false)}
          onCreated={async (world) => {
            setShowCreate(false);
            await reloadWorlds(world.id);
          }}
        />
      )}
    </div>
  );
}

function CreateWorldDialog({ onCancel, onCreated }: {
  onCancel: () => void;
  onCreated: (world: World) => Promise<void>;
}) {
  const [displayName, setDisplayName] = useState('');
  const [images, setImages] = useState<NoteDto[]>([]);
  const [selectedAssetUuid, setSelectedAssetUuid] = useState('');
  const [preview, setPreview] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reloadImages = useCallback(async () => {
    const notes = await api.listNotes();
    setImages(notes.filter((note) => (note.type === 'picture' || note.type === 'drawing') && note.asset));
  }, []);

  useEffect(() => {
    // Initial asynchronous load for the dialog.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    reloadImages().catch((caught) => setError(String(caught)));
  }, [reloadImages]);

  const selectedImage = useMemo(
    () => images.find((image) => image.asset?.uuid === selectedAssetUuid),
    [images, selectedAssetUuid],
  );

  useEffect(() => {
    // Avoid showing the previously selected asset while the next preview loads.
    // eslint-disable-next-line react-hooks/set-state-in-effect
    setPreview(null);
    if (!selectedAssetUuid) return;
    api.assetData(selectedAssetUuid).then(setPreview).catch((caught) => setError(String(caught)));
  }, [selectedAssetUuid]);

  const importImage = async () => {
    const selected = await openDialog({ multiple: false, title: 'Choose world backdrop', filters: IMAGE_FILTER });
    if (!selected || typeof selected !== 'string') return;
    setBusy(true);
    try {
      const note = await api.importImageObject(selected);
      await reloadImages();
      setSelectedAssetUuid(note.asset?.uuid ?? '');
      if (!displayName.trim()) setDisplayName(note.title);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setBusy(false);
    }
  };

  const create = async () => {
    if (!displayName.trim() || !selectedAssetUuid) return;
    setBusy(true);
    try {
      const world = await api.createImageWorld({
        display_name: displayName.trim(),
        backdrop_asset_uuid: selectedAssetUuid,
      });
      await onCreated(world);
    } catch (caught) {
      setError(String(caught));
      setBusy(false);
    }
  };

  return (
    <div className="wv-dialog-backdrop">
      <section className="wv-dialog" role="dialog" aria-modal="true" aria-labelledby="wv-create-title">
        <div className="wv-dialog-heading">
          <h3 id="wv-create-title">Create image world</h3>
          <button onClick={onCancel} aria-label="Close">×</button>
        </div>
        {error && <div className="wv-error-banner">{error}</div>}
        <label>
          Display name
          <input value={displayName} onChange={(event) => setDisplayName(event.target.value)} autoFocus />
        </label>
        <label>
          Backdrop image
          <select value={selectedAssetUuid} onChange={(event) => setSelectedAssetUuid(event.target.value)}>
            <option value="">Choose a Picture or Drawing…</option>
            {images.map((image) => (
              <option key={image.id} value={image.asset?.uuid}>{image.title || image.asset?.original_filename}</option>
            ))}
          </select>
        </label>
        <button className="wv-import-button" onClick={importImage} disabled={busy}>
          Import a new image…
        </button>
        {selectedImage?.asset && (
          <div className="wv-create-preview">
            {preview && <img src={preview} alt="" />}
            <span>{selectedImage.asset.intrinsic_dimensions[0]} × {selectedImage.asset.intrinsic_dimensions[1]}</span>
          </div>
        )}
        <div className="wv-dialog-actions">
          <button onClick={onCancel}>Cancel</button>
          <button className="wv-create-button" onClick={create} disabled={busy || !displayName.trim() || !selectedAssetUuid}>
            {busy ? 'Working…' : 'Create world'}
          </button>
        </div>
      </section>
    </div>
  );
}
