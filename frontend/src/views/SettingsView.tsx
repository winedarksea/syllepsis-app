// Unified settings page (gear icon). Gathers app-level prefs (theme, cloud LLM keys, about) and
// book-level config (privacy, sync, default model, advanced tuning) into one elegant panel.
//
// Three persistence tiers: theme → localStorage (via the store); cloud API keys → OS keychain
// (write-only, status is boolean); book config → _config.yaml via per-section updater commands.
// App-level sections always render; book-level sections only when a book is open.

import { useCallback, useEffect, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { ThemePref } from '../lib/store';
import type {
  BuildInfo, BookConfig, CloudLlmProviderDescriptor, CloudLlmProviderStatus,
  PrivacyConfig, SyncConfig, SearchConfig, CleanupConfig, LlmConfig, ModelRef,
} from '../types';
import {
  allThemes, themeById, themeSwatches, themeToJson, normalizeImportedTheme, BUILTIN_THEMES,
} from '../theme/themes';
import { Icon } from '../components/Icon';
import './SettingsView.css';

const THEME_OPTIONS: { value: ThemePref; icon: string; label: string }[] = [
  { value: 'light', icon: 'light_mode', label: 'Light' },
  { value: 'dark', icon: 'dark_mode', label: 'Dark' },
  { value: 'system', icon: 'contrast', label: 'System' },
];

const LOCAL_PROVIDER = 'local';

interface Props {
  // When opened as a modal on the launch screen, WizardShell supplies the title, so skip the
  // page header here.
  launchMode?: boolean;
}

export function SettingsView({ launchMode = false }: Props) {
  const { book, themePref, setThemePref } = useStore();
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [config, setConfig] = useState<BookConfig | null>(null);
  const [descriptors, setDescriptors] = useState<CloudLlmProviderDescriptor[]>([]);
  const [statuses, setStatuses] = useState<CloudLlmProviderStatus[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);

  const loadCloud = useCallback(async () => {
    const [descs, stats] = await Promise.all([
      api.cloudLlmProviderDescriptors(),
      api.cloudLlmProviderStatuses(),
    ]);
    setDescriptors(descs);
    setStatuses(stats);
  }, []);

  useEffect(() => {
    api.getBuildInfo().then(setBuild).catch((e) => setError(String(e)));
    loadCloud().catch((e) => setError(String(e)));
  }, [loadCloud]);

  useEffect(() => {
    if (!book) { setConfig(null); return; }
    api.getBookConfig().then(setConfig).catch((e) => setError(String(e)));
  }, [book]);

  const flash = useCallback((message: string) => {
    setNotice(message);
    setError(null);
  }, []);

  return (
    <div className={`sv-root ${launchMode ? 'sv-modal' : ''}`}>
      {!launchMode && (
        <div className="sv-header">
          <h2 className="sv-title">Settings</h2>
          <span className="sv-subtitle">Stillingar</span>
        </div>
      )}

      {notice && <div className="sv-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <div className="sv-error" onClick={() => setError(null)}>{error}</div>}

      <div className="sv-scroll">
        {/* ── Appearance ── */}
        <Section title="Appearance" subtitle="Útlit">
          <Field label="Mode" hint="Follows the system color scheme when set to System.">
            <div className="sv-segmented" role="radiogroup" aria-label="Theme mode">
              {THEME_OPTIONS.map((opt) => (
                <button
                  key={opt.value}
                  role="radio"
                  aria-checked={themePref === opt.value}
                  className={`sv-segment ${themePref === opt.value ? 'active' : ''}`}
                  onClick={() => setThemePref(opt.value)}
                >
                  <Icon name={opt.icon} size={16} />
                  <span>{opt.label}</span>
                </button>
              ))}
            </div>
          </Field>
          <ThemePicker onNotice={flash} onError={setError} />
        </Section>

        {/* ── AI & LLM ── */}
        <Section title="AI & Language Models" subtitle="Vélmenni">
          <CloudProvidersPanel
            descriptors={descriptors}
            statuses={statuses}
            onChanged={(msg) => { flash(msg); loadCloud().catch((e) => setError(String(e))); }}
            onError={setError}
          />
          {book ? (
            config && (
              <LlmDefaultsPanel
                config={config}
                providers={descriptors}
                onSaved={(llm) => { setConfig((p) => p && { ...p, llm }); flash('AI defaults saved.'); }}
                onError={setError}
              />
            )
          ) : (
            <p className="sv-locked">Open a book to set its default model and behavior.</p>
          )}
        </Section>

        {/* ── Book-level sections ── */}
        {book ? (
          config && (
            <>
              <Section title="Privacy & Security" subtitle="Vernd">
                <PrivacyPanel
                  value={config.privacy}
                  onSaved={(privacy) => { setConfig((p) => p && { ...p, privacy }); flash('Privacy saved.'); }}
                  onError={setError}
                />
              </Section>

              <Section title="Sync & Backup" subtitle="Samstilling">
                <SyncPanel
                  value={config.sync}
                  onSaved={(sync) => { setConfig((p) => p && { ...p, sync }); flash('Sync saved.'); }}
                  onError={setError}
                />
              </Section>

              <section className="sv-section">
                <button
                  className="sv-disclosure"
                  onClick={() => setAdvancedOpen((v) => !v)}
                  aria-expanded={advancedOpen}
                >
                  <Icon name={advancedOpen ? 'expand_more' : 'chevron_right'} size={18} />
                  <span className="sv-section-title">Advanced</span>
                  <span className="sv-section-subtitle">Djúpstillingar</span>
                </button>
                {advancedOpen && (
                  <div className="sv-section-body">
                    <p className="sv-hint">Tuning knobs for search ranking and cleanup timing. Change these only if you know what you're adjusting.</p>
                    <SearchPanel
                      value={config.search}
                      onSaved={(search) => { setConfig((p) => p && { ...p, search }); flash('Search tuning saved.'); }}
                      onError={setError}
                    />
                    <CleanupPanel
                      value={config.cleanup}
                      onSaved={(cleanup) => { setConfig((p) => p && { ...p, cleanup }); flash('Cleanup saved.'); }}
                      onError={setError}
                    />
                  </div>
                )}
              </section>
            </>
          )
        ) : (
          <Section title="Book Settings" subtitle="Bókarstillingar">
            <p className="sv-locked">Privacy, sync, and advanced tuning are stored per book. Open a book to configure them.</p>
          </Section>
        )}

        {/* ── Plugins (placeholder) ── */}
        <Section title="Plugins" subtitle="Viðbætur">
          <div className="sv-plugins">
            <Icon name="extension" size={22} className="sv-plugins-icon" />
            <div>
              <div className="sv-plugins-title">Coming soon</div>
              <p className="sv-plugins-text">Sandboxed (WASM) extensions will let you add new tools, views, and AI actions. A hosted marketplace for plugins, themes, and knowledge packs is planned.</p>
            </div>
          </div>
        </Section>

        {/* ── About ── */}
        <Section title="About" subtitle="Um Syllepsis">
          <div className="sv-about">
            <div className="sv-about-mark">S</div>
            <div className="sv-about-text">
              <div className="sv-about-name">Syllepsis</div>
              <div className="sv-about-meta">
                Version {build?.version ?? '—'} · Built {build?.build_date ?? '—'}
              </div>
              <p className="sv-about-flavor">A local-first knowledge book — your saga, kept on your own hearth.</p>
            </div>
          </div>
        </Section>
      </div>
    </div>
  );
}

// ── Layout primitives ──────────────────────────────────────────────────────────

function Section({ title, subtitle, children }: { title: string; subtitle: string; children: React.ReactNode }) {
  return (
    <section className="sv-section">
      <div className="sv-section-head">
        <h3 className="sv-section-title">{title}</h3>
        <span className="sv-section-subtitle">{subtitle}</span>
      </div>
      <div className="sv-section-body">{children}</div>
    </section>
  );
}

function Field({ label, hint, children }: { label: string; hint?: string; children: React.ReactNode }) {
  return (
    <div className="sv-field">
      <div className="sv-field-label">
        <span>{label}</span>
        {hint && <span className="sv-field-hint">{hint}</span>}
      </div>
      <div className="sv-field-control">{children}</div>
    </div>
  );
}

function SaveBar({ saving, dirty, onSave }: { saving: boolean; dirty: boolean; onSave: () => void }) {
  return (
    <div className="sv-savebar">
      <button className="sv-btn sv-btn-primary" disabled={saving || !dirty} onClick={onSave}>
        {saving ? 'Saving…' : 'Save'}
      </button>
    </div>
  );
}

// Small hook: a section's draft + dirty + save lifecycle around an updater command.
function useSectionDraft<T>(value: T, save: (draft: T) => Promise<void>, onError: (m: string) => void) {
  const [draft, setDraft] = useState<T>(value);
  const [saving, setSaving] = useState(false);
  useEffect(() => { setDraft(value); }, [value]);
  const dirty = JSON.stringify(draft) !== JSON.stringify(value);
  const commit = useCallback(async () => {
    setSaving(true);
    try { await save(draft); }
    catch (e) { onError(String(e)); }
    finally { setSaving(false); }
  }, [draft, save, onError]);
  return { draft, setDraft, dirty, saving, commit };
}

function NumberInput({ value, onChange, step }: { value: number; onChange: (n: number) => void; step?: number }) {
  return (
    <input
      type="number"
      className="sv-input sv-input-num"
      value={value}
      step={step ?? 1}
      onChange={(e) => onChange(e.target.value === '' ? 0 : Number(e.target.value))}
    />
  );
}

// ── Theme family picker (app-level) ────────────────────────────────────────────

function ThemePicker({ onNotice, onError }: { onNotice: (m: string) => void; onError: (m: string) => void }) {
  const { theme, themeId, customThemes, setThemeId, addCustomTheme, removeCustomTheme } = useStore();
  const themes = allThemes(customThemes);

  const importTheme = useCallback(async () => {
    const selected = await openDialog({
      multiple: false,
      title: 'Import theme file',
      filters: [{ name: 'Theme', extensions: ['json'] }],
    });
    if (!selected || typeof selected !== 'string') return;
    try {
      const text = await api.readTextImportFile(selected);
      const { theme: imported, error } = normalizeImportedTheme(text);
      if (error || !imported) { onError(error ?? 'Could not read theme.'); return; }
      addCustomTheme(imported);
      onNotice(`Imported theme "${imported.name}".`);
    } catch (e) {
      onError(String(e));
    }
  }, [addCustomTheme, onNotice, onError]);

  const copyTemplate = useCallback(async () => {
    const active = themeById(themeId, customThemes) ?? BUILTIN_THEMES[0];
    try {
      await navigator.clipboard.writeText(themeToJson(active));
      onNotice('Current theme copied as a JSON template — edit it and import.');
    } catch (e) {
      onError(String(e));
    }
  }, [themeId, customThemes, onNotice, onError]);

  return (
    <div className="sv-themes">
      <div className="sv-themes-head">
        <div className="sv-field-label">
          <span>Theme</span>
          <span className="sv-field-hint">Each theme brings its own light & dark palette. Import a JSON theme file, or copy the current theme as a starting template.</span>
        </div>
        <div className="sv-themes-actions">
          <button className="sv-btn" onClick={copyTemplate}>Copy template</button>
          <button className="sv-btn sv-btn-primary" onClick={importTheme}>Import theme…</button>
        </div>
      </div>
      <div className="sv-theme-grid">
        {themes.map((t) => (
          <button
            key={t.id}
            className={`sv-theme-card ${t.id === themeId ? 'active' : ''}`}
            onClick={() => setThemeId(t.id)}
            aria-pressed={t.id === themeId}
          >
            <div className="sv-swatches">
              {themeSwatches(t, theme).map((color, i) => (
                <span key={i} className="sv-swatch" style={{ background: color }} />
              ))}
            </div>
            <div className="sv-theme-meta">
              <span className="sv-theme-name">{t.name}</span>
              {t.builtin ? (
                <span className="sv-theme-tag">Built-in</span>
              ) : t.author ? (
                <span className="sv-theme-tag">by {t.author}</span>
              ) : null}
            </div>
            {!t.builtin && (
              <span
                className="sv-theme-delete"
                role="button"
                tabIndex={0}
                title="Delete theme"
                aria-label={`Delete ${t.name}`}
                onClick={(e) => { e.stopPropagation(); removeCustomTheme(t.id); onNotice(`Removed "${t.name}".`); }}
                onKeyDown={(e) => { if (e.key === 'Enter') { e.stopPropagation(); removeCustomTheme(t.id); } }}
              >
                <Icon name="delete" size={15} />
              </span>
            )}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Cloud providers (device-level keychain) ────────────────────────────────────

function CloudProvidersPanel({ descriptors, statuses, onChanged, onError }: {
  descriptors: CloudLlmProviderDescriptor[];
  statuses: CloudLlmProviderStatus[];
  onChanged: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [urls, setUrls] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);

  const statusFor = (provider: string) => statuses.find((s) => s.provider === provider);

  const save = useCallback(async (descriptor: CloudLlmProviderDescriptor) => {
    const provider = descriptor.provider;
    const apiKey = keys[provider]?.trim();
    const baseUrl = urls[provider]?.trim();
    if (!apiKey && !baseUrl) return;
    setBusy(provider);
    try {
      // null leaves the stored value unchanged; only send fields the user actually typed.
      await api.saveCloudLlmProviderSettings({
        provider,
        api_key: apiKey ? apiKey : null,
        base_url: baseUrl ? baseUrl : null,
      });
      setKeys((k) => ({ ...k, [provider]: '' }));
      setUrls((u) => ({ ...u, [provider]: '' }));
      onChanged(`${descriptor.display_name} credentials saved.`);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  }, [keys, urls, onChanged, onError]);

  const clear = useCallback(async (descriptor: CloudLlmProviderDescriptor) => {
    setBusy(descriptor.provider);
    try {
      await api.clearCloudLlmProviderSettings(descriptor.provider);
      onChanged(`${descriptor.display_name} credentials cleared.`);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  }, [onChanged, onError]);

  return (
    <div className="sv-providers">
      <p className="sv-hint">API keys are stored in your operating system keychain, never written to the book or synced. They're shown as configured/not — the keys themselves are never displayed.</p>
      {descriptors.map((d) => {
        const status = statusFor(d.provider);
        const configured = status?.api_key_configured || status?.base_url_configured;
        return (
          <div key={d.provider} className="sv-provider">
            <div className="sv-provider-head">
              <span className="sv-provider-name">{d.display_name}</span>
              <span className={`sv-badge ${configured ? 'ok' : 'off'}`}>
                {configured ? 'Configured' : 'Not configured'}
              </span>
            </div>
            <div className="sv-provider-fields">
              <input
                type="password"
                className="sv-input"
                placeholder={status?.api_key_configured ? 'API key set — type to replace' : 'API key'}
                value={keys[d.provider] ?? ''}
                onChange={(e) => setKeys((k) => ({ ...k, [d.provider]: e.target.value }))}
              />
              {d.base_url_required && (
                <input
                  type="text"
                  className="sv-input"
                  placeholder={status?.base_url_configured ? 'Base URL set — type to replace' : 'Base URL (e.g. http://localhost:8080/v1)'}
                  value={urls[d.provider] ?? ''}
                  onChange={(e) => setUrls((u) => ({ ...u, [d.provider]: e.target.value }))}
                />
              )}
            </div>
            <div className="sv-savebar">
              <button
                className="sv-btn sv-btn-primary"
                disabled={busy === d.provider || (!keys[d.provider]?.trim() && !urls[d.provider]?.trim())}
                onClick={() => save(d)}
              >
                {busy === d.provider ? 'Saving…' : 'Save'}
              </button>
              {configured && (
                <button className="sv-btn" disabled={busy === d.provider} onClick={() => clear(d)}>
                  Clear
                </button>
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Default model & behavior (book-level llm config) ────────────────────────────

function LlmDefaultsPanel({ config, providers, onSaved, onError }: {
  config: BookConfig;
  providers: CloudLlmProviderDescriptor[];
  onSaved: (llm: LlmConfig) => void;
  onError: (message: string) => void;
}) {
  const llm = config.llm;
  const [enabled, setEnabled] = useState(llm.enabled);
  const [autoAccept, setAutoAccept] = useState(llm.auto_accept);
  const [maxTokens, setMaxTokens] = useState(llm.max_new_tokens);
  const [provider, setProvider] = useState(llm.provider);
  // Representative model used by all routes; the bundled id stands in for "local".
  const [model, setModel] = useState(llm.provider === LOCAL_PROVIDER ? '' : llm.routing.summarize.model);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    setEnabled(llm.enabled);
    setAutoAccept(llm.auto_accept);
    setMaxTokens(llm.max_new_tokens);
    setProvider(llm.provider);
    setModel(llm.provider === LOCAL_PROVIDER ? '' : llm.routing.summarize.model);
  }, [llm]);

  const save = useCallback(async () => {
    const isLocal = provider === LOCAL_PROVIDER;
    const ref: ModelRef = { provider, model: isLocal ? llm.local_model : model.trim() };
    const next: LlmConfig = {
      ...llm,
      enabled,
      auto_accept: autoAccept,
      max_new_tokens: maxTokens,
      provider,
      routing: {
        summarize: ref, fact_check: ref, devils_advocate: ref,
        grammar: ref, category_suggest: ref, rewrite: ref,
      },
    };
    setSaving(true);
    try {
      const updated = await api.updateLlmConfig(next);
      onSaved(updated.llm);
    } catch (e) {
      onError(String(e));
    } finally {
      setSaving(false);
    }
  }, [provider, model, enabled, autoAccept, maxTokens, llm, onSaved, onError]);

  const isLocal = provider === LOCAL_PROVIDER;
  const dirty =
    enabled !== llm.enabled || autoAccept !== llm.auto_accept || maxTokens !== llm.max_new_tokens ||
    provider !== llm.provider || (!isLocal && model.trim() !== llm.routing.summarize.model);

  return (
    <div className="sv-subpanel">
      <Field label="AI features" hint="Master switch for all summarize, fact-check, rewrite, and other AI actions.">
        <Toggle checked={enabled} onChange={setEnabled} />
      </Field>
      <Field label="Default model" hint="The default for every AI action. Individual tools (fact-check, rewrite…) can override it when you run them.">
        <div className="sv-inline">
          <select className="sv-input" value={provider} onChange={(e) => setProvider(e.target.value)}>
            <option value={LOCAL_PROVIDER}>Local (bundled model)</option>
            {providers.map((p) => (
              <option key={p.provider} value={p.provider}>{p.display_name}</option>
            ))}
          </select>
          {!isLocal && (
            <input
              className="sv-input"
              placeholder="Model name (e.g. claude-sonnet-4-6)"
              value={model}
              onChange={(e) => setModel(e.target.value)}
            />
          )}
        </div>
      </Field>
      <Field label="Auto-accept proposals" hint="Apply generated proposals immediately instead of queuing them for review.">
        <Toggle checked={autoAccept} onChange={setAutoAccept} />
      </Field>
      <Field label="Max new tokens" hint="Upper bound on local generation length (bounds latency on CPU).">
        <NumberInput value={maxTokens} onChange={setMaxTokens} step={64} />
      </Field>
      <SaveBar saving={saving} dirty={dirty} onSave={save} />
    </div>
  );
}

// ── Privacy ─────────────────────────────────────────────────────────────────────

function PrivacyPanel({ value, onSaved, onError }: {
  value: PrivacyConfig; onSaved: (v: PrivacyConfig) => void; onError: (m: string) => void;
}) {
  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (d) => { const updated = await api.updatePrivacyConfig(d); onSaved(updated.privacy); },
    onError,
  );
  return (
    <div className="sv-subpanel">
      <Field label="Unlock delay (hours)" hint="Wait before a proposed rewrite to a locked note may merge.">
        <NumberInput value={draft.unlock_delay_hours} onChange={(n) => setDraft({ ...draft, unlock_delay_hours: n })} />
      </Field>
      <Field label="Confirmation delay (hours)" hint="Wait before a delete or unlock confirmation takes effect.">
        <NumberInput value={draft.confirmation_delay_hours} onChange={(n) => setDraft({ ...draft, confirmation_delay_hours: n })} />
      </Field>
      <SaveBar saving={saving} dirty={dirty} onSave={commit} />
    </div>
  );
}

// ── Sync ────────────────────────────────────────────────────────────────────────

function SyncPanel({ value, onSaved, onError }: {
  value: SyncConfig; onSaved: (v: SyncConfig) => void; onError: (m: string) => void;
}) {
  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (d) => { const updated = await api.updateSyncConfig(d); onSaved(updated.sync); },
    onError,
  );
  return (
    <div className="sv-subpanel">
      <Field label="Sync enabled" hint="When off, edits stay local and no CRDT sidecars are written.">
        <Toggle checked={draft.enabled} onChange={(b) => setDraft({ ...draft, enabled: b })} />
      </Field>
      <Field label="Merge strategy" hint="LWW is always available; Loro adds character-level text merge (requires the loro build feature).">
        <select className="sv-input" value={draft.crdt_backend} onChange={(e) => setDraft({ ...draft, crdt_backend: e.target.value })}>
          <option value="lww">Last-writer-wins (LWW)</option>
          <option value="loro">Loro (fine-grained)</option>
        </select>
      </Field>
      <Field label="Conflict marker" hint="Filename marker for conflict copies: {name}.{marker}-{actor}.{ext}.">
        <input className="sv-input" value={draft.conflict_marker} onChange={(e) => setDraft({ ...draft, conflict_marker: e.target.value })} />
      </Field>
      <Field label="External-edit skew (seconds)" hint="Clock-skew guard for detecting edits made outside the app.">
        <NumberInput value={draft.external_edit_skew_secs} onChange={(n) => setDraft({ ...draft, external_edit_skew_secs: n })} />
      </Field>
      <SaveBar saving={saving} dirty={dirty} onSave={commit} />
    </div>
  );
}

// ── Advanced: search tuning ──────────────────────────────────────────────────────

function SearchPanel({ value, onSaved, onError }: {
  value: SearchConfig; onSaved: (v: SearchConfig) => void; onError: (m: string) => void;
}) {
  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (d) => { const updated = await api.updateSearchConfig(d); onSaved(updated.search); },
    onError,
  );
  const set = (patch: Partial<SearchConfig>) => setDraft({ ...draft, ...patch });
  return (
    <div className="sv-subpanel">
      <h4 className="sv-subhead">Search ranking</h4>
      <Field label="RRF constant (k)"><NumberInput value={draft.rrf_k} step={1} onChange={(n) => set({ rrf_k: n })} /></Field>
      <Field label="Category upweight"><NumberInput value={draft.category_upweight} step={0.05} onChange={(n) => set({ category_upweight: n })} /></Field>
      <Field label="BM25 k1"><NumberInput value={draft.bm25_k1} step={0.1} onChange={(n) => set({ bm25_k1: n })} /></Field>
      <Field label="BM25 b"><NumberInput value={draft.bm25_b} step={0.05} onChange={(n) => set({ bm25_b: n })} /></Field>
      <Field label="Result limit"><NumberInput value={draft.result_limit} onChange={(n) => set({ result_limit: n })} /></Field>
      <Field label="Related notes limit"><NumberInput value={draft.related_limit} onChange={(n) => set({ related_limit: n })} /></Field>
      <Field label="Duplicate similarity"><NumberInput value={draft.duplicate_similarity} step={0.01} onChange={(n) => set({ duplicate_similarity: n })} /></Field>
      <Field label="Blind-spot similarity"><NumberInput value={draft.blind_spot_similarity} step={0.01} onChange={(n) => set({ blind_spot_similarity: n })} /></Field>
      <SaveBar saving={saving} dirty={dirty} onSave={commit} />
    </div>
  );
}

// ── Advanced: cleanup timing ─────────────────────────────────────────────────────

function CleanupPanel({ value, onSaved, onError }: {
  value: CleanupConfig; onSaved: (v: CleanupConfig) => void; onError: (m: string) => void;
}) {
  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (d) => { const updated = await api.updateCleanupConfig(d); onSaved(updated.cleanup); },
    onError,
  );
  return (
    <div className="sv-subpanel">
      <h4 className="sv-subhead">Cleanup & retention</h4>
      <Field label="Default vanish (days)" hint="Lifespan of a vanishing note set at creation."><NumberInput value={draft.default_vanish_days} onChange={(n) => setDraft({ ...draft, default_vanish_days: n })} /></Field>
      <Field label="Deletion delay (days)" hint="Grace period between mark-for-deletion and permanent removal."><NumberInput value={draft.deletion_delay_days} onChange={(n) => setDraft({ ...draft, deletion_delay_days: n })} /></Field>
      <Field label="Todo archive (days)" hint="How long a done/cancelled todo lingers before archiving."><NumberInput value={draft.todo_archive_days} onChange={(n) => setDraft({ ...draft, todo_archive_days: n })} /></Field>
      <SaveBar saving={saving} dirty={dirty} onSave={commit} />
    </div>
  );
}

// ── Toggle ───────────────────────────────────────────────────────────────────────

function Toggle({ checked, onChange }: { checked: boolean; onChange: (b: boolean) => void }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      className={`sv-toggle ${checked ? 'on' : ''}`}
      onClick={() => onChange(!checked)}
    >
      <span className="sv-toggle-knob" />
    </button>
  );
}
