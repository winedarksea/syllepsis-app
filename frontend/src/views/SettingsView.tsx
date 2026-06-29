// Unified settings page (gear icon). Gathers app-level prefs (theme, cloud LLM keys, about) and
// book-level config (privacy, sync, default model, advanced tuning) into one elegant panel.
//
// Three persistence tiers: theme → localStorage (via the store); cloud API keys → OS keychain
// (read only for explicit credential actions); book config → _config.yaml via section updaters.
// App-level sections always render; book-level sections only when a book is open.

import { useCallback, useEffect, useState } from 'react';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { listen } from '@tauri-apps/api/event';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { ThemePref } from '../lib/store';
import type {
  BuildInfo, BookConfig, CloudLlmProviderDescriptor,
  PrivacyConfig, SyncConfig, SearchConfig, CleanupConfig, LlmConfig, ModelRef,
  EmbeddingConfig, LocalAiDevicePolicy, ModelManifest,
  CloudSyncProviderDescriptor, CloudSyncProviderStatus, PluginDescriptor,
  SyncReport, CloudSyncFinished, DeleteCurrentBookReport,
  SearchApiStatus,
} from '../types';
import {
  allThemes, themeById, themeSwatches, themeToJson, normalizeImportedTheme, BUILTIN_THEMES,
  resolveThemeStyle, type Theme,
} from '../theme/themes';
import { getIconSet } from '../theme/icons/sets';
import { Icon, useThemeStyle } from '../components/Icon';
import { PageHeader } from '../components/PageHeader';
import { CloudLlmModelPicker } from '../components/CloudLlmModelPicker';
import './SettingsView.css';

// Evocative section sub-text, varying by the active theme's flavor language.
const SUBTITLES = {
  settings:   { icelandic: 'Stillingar',      latin: 'Ordinatio' },
  appearance: { icelandic: 'Útlit',           latin: 'Aspectus' },
  ai:         { icelandic: 'Vélmenni',        latin: 'Machina' },
  privacy:    { icelandic: 'Vernd',           latin: 'Seclusio' },
  sync:       { icelandic: 'Samstilling',     latin: 'Concordia' },
  advanced:   { icelandic: 'Djúpstillingar',  latin: 'Profunda' },
  book:       { icelandic: 'Bókarstillingar', latin: 'Codex' },
  plugins:    { icelandic: 'Viðbætur',        latin: 'Additamenta' },
  api:        { icelandic: 'Leitarviðmót',   latin: 'Retrimentum' },
  about:      { icelandic: 'Um Syllepsis',    latin: 'De Syllepsi' },
  delete:     { icelandic: 'Eyða bók',        latin: 'Delere' },
} as const;

const THEME_OPTIONS: { value: ThemePref; icon: string; label: string }[] = [
  { value: 'light', icon: 'light_mode', label: 'Light' },
  { value: 'dark', icon: 'dark_mode', label: 'Dark' },
  { value: 'system', icon: 'contrast', label: 'System' },
];

const LOCAL_PROVIDER = 'local';
const LORO_URL = 'https://github.com/loro-dev/loro';

interface Props {
  // When opened as a modal on the launch screen, WizardShell supplies the title, so skip the
  // page header here.
  launchMode?: boolean;
}

export function SettingsView({ launchMode = false }: Props) {
  const { book, themePref, setThemePref, closeBook, hideUnsortedBadge, setHideUnsortedBadge } = useStore();
  const { flavorLang } = useThemeStyle();
  const [build, setBuild] = useState<BuildInfo | null>(null);
  const [config, setConfig] = useState<BookConfig | null>(null);
  const [descriptors, setDescriptors] = useState<CloudLlmProviderDescriptor[]>([]);
  const [plugins, setPlugins] = useState<PluginDescriptor[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [localAiPolicy, setLocalAiPolicy] = useState<LocalAiDevicePolicy | null>(null);
  const [embeddingModels, setEmbeddingModels] = useState<ModelManifest[]>([]);
  const [searchApiStatus, setSearchApiStatus] = useState<SearchApiStatus | null>(null);

  const reportError = useCallback((message: string) => {
    setNotice(null);
    setError(message);
  }, []);

  const loadCloud = useCallback(async () => {
    setDescriptors(await api.cloudLlmProviderDescriptors());
  }, []);

  useEffect(() => {
    api.getBuildInfo().then(setBuild).catch((e) => reportError(String(e)));
    loadCloud().catch((e) => reportError(String(e)));
    api.listPlugins().then(setPlugins).catch((e) => reportError(String(e)));
    api.getLocalAiDevicePolicy().then(setLocalAiPolicy).catch((e) => reportError(String(e)));
    api.builtinModelManifests()
      .then((manifests) => setEmbeddingModels(manifests.filter((manifest) => manifest.kind === 'embedding')))
      .catch((e) => reportError(String(e)));
    api.searchApiStatus().then(setSearchApiStatus).catch(() => undefined);
  }, [loadCloud, reportError]);

  useEffect(() => {
    if (!book) { setConfig(null); return; }
    api.getBookConfig().then(setConfig).catch((e) => reportError(String(e)));
  }, [book, reportError]);

  const flash = useCallback((message: string) => {
    setNotice(message);
  }, []);

  return (
    <div className={`sv-root ${launchMode ? 'sv-modal' : ''}`}>
      {!launchMode && (
        <PageHeader
          title="Settings"
          secondary={<span className="sv-subtitle">{SUBTITLES.settings[flavorLang]}</span>}
        />
      )}

      {notice && <div className="sv-notice" onClick={() => setNotice(null)}>{notice}</div>}
      {error && <SettingsError message={error} onDismiss={() => setError(null)} />}

      <div className="sv-scroll">
        {/* ── Appearance ── */}
        <Section title="Appearance" subtitle={SUBTITLES.appearance[flavorLang]}>
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
          <ThemePicker onNotice={flash} onError={reportError} />
          <Field label="Show sidebar badges" hint="Displays count badges on the Notebox and Diagnostics sidebar items. Turn off to reduce visual noise.">
            <Toggle checked={!hideUnsortedBadge} onChange={(v) => setHideUnsortedBadge(!v)} />
          </Field>
        </Section>

        {/* ── AI & LLM ── */}
        <Section title="AI & Language Models" subtitle={SUBTITLES.ai[flavorLang]}>
          <CloudProvidersPanel
            descriptors={descriptors}
            onChanged={flash}
            onError={reportError}
          />
          {localAiPolicy && (
            <DeviceEmbeddingPanel
              value={localAiPolicy}
              onSaved={(policy) => {
                setLocalAiPolicy(policy);
                flash('Device embedding policy saved.');
              }}
              onError={reportError}
            />
          )}
          {book ? (
            config && (
              <LlmDefaultsPanel
                config={config}
                providers={descriptors}
                onSaved={(llm) => { setConfig((p) => p && { ...p, llm }); flash('AI defaults saved.'); }}
                onError={reportError}
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
              <Section title="Privacy & Security" subtitle={SUBTITLES.privacy[flavorLang]}>
                <PrivacyPanel
                  value={config.privacy}
                  onSaved={(privacy) => { setConfig((p) => p && { ...p, privacy }); flash('Privacy saved.'); }}
                  onError={reportError}
                />
              </Section>

              <Section title="Sync & Backup" subtitle={SUBTITLES.sync[flavorLang]}>
                <SyncPanel
                  value={config.sync}
                  onSaved={(sync) => { setConfig((p) => p && { ...p, sync }); flash('Sync saved.'); }}
                  onError={reportError}
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
                  <span className="sv-section-subtitle">{SUBTITLES.advanced[flavorLang]}</span>
                </button>
                {advancedOpen && (
                  <div className="sv-section-body">
                    <p className="sv-hint">Tuning knobs for search ranking and cleanup timing. Change these only if you know what you're adjusting.</p>
                    <SearchPanel
                      value={config.search}
                      onSaved={(search) => { setConfig((p) => p && { ...p, search }); flash('Search tuning saved.'); }}
                      onError={reportError}
                    />
                    <EmbeddingPanel
                      value={config.embedding}
                      models={embeddingModels}
                      onSaved={(embedding) => {
                        setConfig((p) => p && { ...p, embedding });
                        flash('Embedding configuration saved.');
                      }}
                      onError={reportError}
                    />
                    <CleanupPanel
                      value={config.cleanup}
                      onSaved={(cleanup) => { setConfig((p) => p && { ...p, cleanup }); flash('Cleanup saved.'); }}
                      onError={reportError}
                    />
                  </div>
                )}
              </section>
            </>
          )
        ) : (
          <Section title="Book Settings" subtitle={SUBTITLES.book[flavorLang]}>
            <p className="sv-locked">Privacy, sync, and advanced tuning are stored per book. Open a book to configure them.</p>
            <DeviceCloudSyncPanel onError={reportError} />
          </Section>
        )}

        {/* ── Plugins ── */}
        <Section title="Plugins" subtitle={SUBTITLES.plugins[flavorLang]}>
          <PluginsPanel
            plugins={plugins}
            onPluginInstalled={(name) => {
              flash(`"${name}" installed — restart Syllepsis to load it.`);
            }}
            onPluginsChanged={() => {
              api.listPlugins().then(setPlugins).catch((e) => reportError(String(e)));
            }}
            onError={reportError}
          />
        </Section>

        {/* ── Local Search API ── */}
        <Section title="Local Search API" subtitle={SUBTITLES.api[flavorLang]}>
          {searchApiStatus ? (
            <SearchApiPanel
              status={searchApiStatus}
              onChanged={setSearchApiStatus}
              onError={reportError}
            />
          ) : (
            <p className="sv-hint">Loading…</p>
          )}
        </Section>

        {/* ── About ── */}
        <Section title="About" subtitle={SUBTITLES.about[flavorLang]}>
          <div className="sv-about">
            <div className="sv-about-mark">S</div>
            <div className="sv-about-text">
              <div className="sv-about-name">Syllepsis</div>
              <div className="sv-about-meta">
                Version {build?.version ?? '—'} · Built {build?.build_date ?? '—'}
              </div>
              <p className="sv-about-flavor">A local-first knowledge book</p>
            </div>
          </div>
        </Section>

        {book && (
          <Section title="Delete Book" subtitle={SUBTITLES.delete[flavorLang]}>
            <DeleteBookPanel
              bookName={book.name}
              onDeleted={(report) => {
                closeBook();
                const failures = report.cloud_cleanup.filter((outcome) => outcome.error);
                if (failures.length === 0) {
                  const successful = report.cloud_cleanup.filter((outcome) => outcome.attempted).length;
                  if (successful > 0) {
                    flash(`Deleted "${report.book_name}". Cloud cleanup completed for ${successful} provider${successful === 1 ? '' : 's'}.`);
                  } else {
                    flash(`Deleted "${report.book_name}".`);
                  }
                  return;
                }
                const failureText = failures
                  .map((failure) => `${failure.provider}: ${failure.error}`)
                  .join(' | ');
                reportError(`Deleted "${report.book_name}" locally. Cloud cleanup failed for ${failures.length} provider${failures.length === 1 ? '' : 's'}: ${failureText}`);
              }}
              onError={reportError}
            />
          </Section>
        )}
      </div>
    </div>
  );
}

// ── Layout primitives ──────────────────────────────────────────────────────────

function SettingsError({ message, onDismiss }: { message: string; onDismiss: () => void }) {
  const copyError = useCallback(async () => {
    await navigator.clipboard?.writeText(message).catch(() => undefined);
  }, [message]);

  return (
    <div className="sv-error-panel" role="alert">
      <div className="sv-error-panel-head">
        <div className="sv-error-panel-title">
          <Icon name="error" size={18} />
          <span>Error</span>
        </div>
        <div className="sv-error-panel-actions">
          <button className="sv-btn sv-btn-compact" type="button" onClick={copyError}>Copy</button>
          <button className="sv-btn sv-btn-compact" type="button" onClick={onDismiss}>Dismiss</button>
        </div>
      </div>
      <pre className="sv-error-panel-message">{message}</pre>
    </div>
  );
}

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

function Field({ label, hint, children }: { label: string; hint?: React.ReactNode; children: React.ReactNode }) {
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

function SaveBar({ saving, dirty, disabled = false, onSave }: { saving: boolean; dirty: boolean; disabled?: boolean; onSave: () => void }) {
  return (
    <div className="sv-savebar">
      <button className="sv-btn sv-btn-primary" disabled={saving || disabled || !dirty} onClick={onSave}>
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

// Renders 2–3 signature glyphs from a theme card's own icon set (not the active theme).
// Uses a simplified inline SVG — no store access, no hook — so card previews are independent.
function CardIcons({ t, customThemes }: { t: Theme; customThemes: Theme[] }) {
  const style = resolveThemeStyle(t.id, customThemes);
  const set = getIconSet(style.iconSet);
  const slots = (['graph', 'worlds', 'book'] as const).filter((s) => set[s]);
  if (slots.length === 0) return null;
  return (
    <div className="sv-card-icons">
      {slots.map((s) => {
        const icon = set[s]!;
        const paths = Array.isArray(icon.path) ? icon.path : [icon.path];
        return (
          <svg
            key={s}
            viewBox={icon.viewBox ?? '0 0 24 24'}
            width={16}
            height={16}
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden
          >
            {paths.map((d, i) => <path key={i} d={d} />)}
          </svg>
        );
      })}
    </div>
  );
}

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
          <span className="sv-field-hint">Each theme brings its own palette, visual style (graph edges, node shapes, dividers, grid), and signature icons. Import a JSON theme file, or copy the current theme as a starting template.</span>
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
              <CardIcons t={t} customThemes={customThemes} />
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

function CloudProvidersPanel({ descriptors, onChanged, onError }: {
  descriptors: CloudLlmProviderDescriptor[];
  onChanged: (message: string) => void;
  onError: (message: string) => void;
}) {
  const [keys, setKeys] = useState<Record<string, string>>({});
  const [urls, setUrls] = useState<Record<string, string>>({});
  const [knownConfigured, setKnownConfigured] = useState<Record<string, boolean | undefined>>({});
  const [busy, setBusy] = useState<{ provider: string; operation: 'save' | 'clear' | 'test' } | null>(null);
  const [connectionFeedback, setConnectionFeedback] = useState<Record<string, {
    kind: 'success' | 'warning' | 'error';
    message: string;
  } | undefined>>({});

  const clearConnectionFeedback = useCallback((provider: string) => {
    setConnectionFeedback((current) => ({ ...current, [provider]: undefined }));
  }, []);

  const save = useCallback(async (descriptor: CloudLlmProviderDescriptor) => {
    const provider = descriptor.provider;
    const apiKey = keys[provider]?.trim();
    const baseUrl = urls[provider]?.trim();
    if (!apiKey && !baseUrl) return;
    setBusy({ provider, operation: 'save' });
    try {
      // null leaves the stored value unchanged; only send fields the user actually typed.
      await api.saveCloudLlmProviderSettings({
        provider,
        api_key: apiKey ? apiKey : null,
        base_url: baseUrl ? baseUrl : null,
      });
      setKeys((k) => ({ ...k, [provider]: '' }));
      setUrls((u) => ({ ...u, [provider]: '' }));
      setKnownConfigured((current) => ({ ...current, [provider]: true }));
      clearConnectionFeedback(provider);
      onChanged(`${descriptor.display_name} credentials saved.`);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  }, [clearConnectionFeedback, keys, urls, onChanged, onError]);

  const clear = useCallback(async (descriptor: CloudLlmProviderDescriptor) => {
    setBusy({ provider: descriptor.provider, operation: 'clear' });
    try {
      await api.clearCloudLlmProviderSettings(descriptor.provider);
      setKnownConfigured((current) => ({ ...current, [descriptor.provider]: false }));
      clearConnectionFeedback(descriptor.provider);
      onChanged(`${descriptor.display_name} credentials cleared.`);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(null);
    }
  }, [clearConnectionFeedback, onChanged, onError]);

  const testConnection = useCallback(async (descriptor: CloudLlmProviderDescriptor) => {
    const provider = descriptor.provider;
    setBusy({ provider, operation: 'test' });
    clearConnectionFeedback(provider);
    try {
      const result = await api.testCloudLlmProviderConnection({
        provider,
        api_key: keys[provider]?.trim() || null,
        base_url: urls[provider]?.trim() || null,
      });
      const authenticationFeedback = {
        verified: {
          kind: 'success' as const,
          message: `Authenticated · ${result.model_count} model${result.model_count === 1 ? '' : 's'} reported`,
        },
        not_required: {
          kind: 'warning' as const,
          message: `Connected · ${result.model_count} models reported, but the endpoint also works without the API key`,
        },
        not_tested: {
          kind: 'success' as const,
          message: `Connected without authentication · ${result.model_count} model${result.model_count === 1 ? '' : 's'} reported`,
        },
        inconclusive: {
          kind: 'warning' as const,
          message: `Connected · ${result.model_count} models reported; authentication enforcement could not be confirmed`,
        },
      }[result.authentication_status];
      setKnownConfigured((current) => ({ ...current, [provider]: true }));
      setConnectionFeedback((current) => ({
        ...current,
        [provider]: authenticationFeedback,
      }));
    } catch (e) {
      setConnectionFeedback((current) => ({
        ...current,
        [provider]: { kind: 'error', message: String(e) },
      }));
    } finally {
      setBusy(null);
    }
  }, [clearConnectionFeedback, keys, urls]);

  return (
    <div className="sv-providers">
      <p className="sv-hint">API keys are stored in your operating system keychain, never written to the book or synced. Test connection checks the entered or saved credentials by listing models; it does not generate text or consume model tokens.</p>
      {descriptors.map((d) => {
        const configured = knownConfigured[d.provider];
        const feedback = connectionFeedback[d.provider];
        return (
          <div key={d.provider} className="sv-provider">
            <div className="sv-provider-head">
              <span className="sv-provider-name">{d.display_name}</span>
              <span className={`sv-badge ${configured === true ? 'ok' : 'off'}`}>
                {configured === true ? 'Configured' : configured === false ? 'Cleared' : 'Keychain-backed'}
              </span>
            </div>
            <div className="sv-provider-fields">
              <input
                type="password"
                className="sv-input"
                placeholder="API key (leave blank to use a saved key)"
                value={keys[d.provider] ?? ''}
                onChange={(e) => {
                  setKeys((k) => ({ ...k, [d.provider]: e.target.value }));
                  clearConnectionFeedback(d.provider);
                }}
              />
              {d.base_url_required && (
                <input
                  type="text"
                  className="sv-input"
                  placeholder="Base URL (leave blank to use a saved URL)"
                  value={urls[d.provider] ?? ''}
                  onChange={(e) => {
                    setUrls((u) => ({ ...u, [d.provider]: e.target.value }));
                    clearConnectionFeedback(d.provider);
                  }}
                />
              )}
            </div>
            <div className="sv-provider-footer">
              <div
                className={`sv-connection-feedback ${feedback?.kind ?? ''}`}
                role="status"
                aria-live="polite"
              >
                {feedback && (
                  <>
                    <Icon
                      name={feedback.kind === 'success' ? 'check_circle' : feedback.kind === 'warning' ? 'info' : 'error'}
                      size={15}
                    />
                    <span>{feedback.message}</span>
                  </>
                )}
              </div>
              <div className="sv-savebar">
                <button
                  className="sv-btn sv-btn-test"
                  disabled={busy !== null}
                  onClick={() => testConnection(d)}
                >
                  <Icon name="network_check" size={16} />
                  {busy?.provider === d.provider && busy.operation === 'test' ? 'Testing…' : 'Test connection'}
                </button>
                <button
                  className="sv-btn sv-btn-primary"
                  disabled={busy !== null || (!keys[d.provider]?.trim() && !urls[d.provider]?.trim())}
                  onClick={() => save(d)}
                >
                  {busy?.provider === d.provider && busy.operation === 'save' ? 'Saving…' : 'Save'}
                </button>
                <button className="sv-btn" disabled={busy !== null} onClick={() => clear(d)}>
                  {busy?.provider === d.provider && busy.operation === 'clear' ? 'Clearing…' : 'Clear saved'}
                </button>
              </div>
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ── Default model & behavior (book-level llm config) ────────────────────────────

export function LlmDefaultsPanel({ config, providers, onSaved, onError }: {
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
    if (!isLocal && !model.trim()) return;
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
            <CloudLlmModelPicker
              key={provider}
              provider={provider}
              value={model}
              onChange={setModel}
              selectClassName="sv-input"
              inputClassName="sv-input"
              modelPlaceholder="Model name (e.g. claude-sonnet-4-6)"
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
      <SaveBar saving={saving} dirty={dirty} disabled={!isLocal && !model.trim()} onSave={save} />
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

function upsertCloudProviderStatus(
  statuses: CloudSyncProviderStatus[],
  nextStatus: CloudSyncProviderStatus,
): CloudSyncProviderStatus[] {
  const replacedStatuses = statuses.map((status) =>
    status.provider === nextStatus.provider ? nextStatus : status
  );
  return replacedStatuses.some((status) => status.provider === nextStatus.provider)
    ? replacedStatuses
    : [...statuses, nextStatus];
}

function markActiveCloudProvider(
  statuses: CloudSyncProviderStatus[],
  activeProvider: string,
): CloudSyncProviderStatus[] {
  return statuses.map((status) => ({
    ...status,
    active_for_current_book: status.provider === activeProvider,
  }));
}

function cloudSyncReportSummary(report: SyncReport): string {
  return [
    `Cloud sync complete. ${report.pushed.length} pushed`,
    `${report.pulled.length} pulled`,
    `${report.merged.length} merged`,
    `${report.conflicted.length} conflicted`,
    `${report.deleted_local.length + report.deleted_remote.length} deleted`,
    `${report.skipped} unchanged.`,
  ].join(', ');
}

function DeviceCloudSyncPanel({ onError }: { onError: (m: string) => void }) {
  const [cloudProviders, setCloudProviders] = useState<CloudSyncProviderDescriptor[]>([]);
  const [cloudStatuses, setCloudStatuses] = useState<CloudSyncProviderStatus[]>([]);
  const [busy, setBusy] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<Record<string, string>>({});

  const loadCloud = useCallback(async () => {
    const [providers, statuses] = await Promise.all([
      api.cloudSyncProviderDescriptors(),
      api.cloudSyncProviderStatuses(),
    ]);
    setCloudProviders(providers);
    setCloudStatuses(statuses);
  }, []);

  useEffect(() => {
    loadCloud().catch((e) => onError(String(e)));
  }, [loadCloud, onError]);

  useEffect(() => {
    let unlistenCompleted: (() => void) | undefined;
    let unlistenFailed: (() => void) | undefined;
    let disposed = false;

    Promise.all([
      listen<CloudSyncProviderStatus>('cloud-sync://oauth-completed', (event) => {
        setCloudStatuses((prev) => upsertCloudProviderStatus(prev, event.payload));
        setFeedback((current) => ({
          ...current,
          [event.payload.provider]: 'Authorization complete.',
        }));
        setBusy(null);
      }),
      listen<string>('cloud-sync://oauth-failed', (event) => {
        setBusy(null);
        onError(event.payload);
      }),
    ]).then(([completed, failed]) => {
      if (disposed) {
        completed();
        failed();
        return;
      }
      unlistenCompleted = completed;
      unlistenFailed = failed;
    }).catch((error) => onError(String(error)));

    return () => {
      disposed = true;
      unlistenCompleted?.();
      unlistenFailed?.();
    };
  }, [onError]);

  const cloudStatus = (provider: string) => cloudStatuses.find((status) => status.provider === provider);

  const connectCloud = useCallback(async (provider: string) => {
    setBusy(provider);
    try {
      const start = await api.connectCloudSyncProvider(provider);
      await openUrl(start.auth_url);
    } catch (e) {
      setBusy(null);
      onError(String(e));
    }
  }, [onError]);

  const disconnectCloud = useCallback(async (provider: string) => {
    setBusy(provider);
    try {
      const status = await api.disconnectCloudSyncProvider(provider);
      setCloudStatuses((current) => upsertCloudProviderStatus(current, status));
      setFeedback((current) => {
        const next = { ...current };
        delete next[provider];
        return next;
      });
    } catch (e) { onError(String(e)); }
    finally { setBusy(null); }
  }, [onError]);

  return (
    <div className="sv-subpanel">
      <div className="sv-subhead">Cloud Sync Accounts</div>
      <p className="sv-hint">Authorize cloud accounts here, then use Load from Cloud on the launch screen to open an existing notebook.</p>
      <div className="sv-providers">
        {cloudProviders.map((provider) => {
          const status = cloudStatus(provider.provider);
          return (
            <div key={provider.provider} className="sv-provider">
              <div className="sv-provider-head">
                <span className="sv-provider-name">{provider.display_name}</span>
                <span className={`sv-pill ${status?.connected ? 'ok' : ''}`}>
                  {status?.connected ? 'Connected' : status ? 'Disconnected' : 'Keychain-backed'}
                </span>
              </div>
              <div className="sv-actions">
                <button className="sv-btn" disabled={busy === provider.provider} onClick={() => connectCloud(provider.provider)}>
                  {status?.connected ? 'Reconnect' : 'Authorize / Reauthorize'}
                </button>
                <button className="sv-btn" disabled={busy === provider.provider} onClick={() => disconnectCloud(provider.provider)}>Disconnect saved</button>
              </div>
              {feedback[provider.provider] && (
                <p className="sv-connection-feedback success">{feedback[provider.provider]}</p>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function SyncPanel({ value, onSaved, onError }: {
  value: SyncConfig; onSaved: (v: SyncConfig) => void; onError: (m: string) => void;
}) {
  const [cloudProviders, setCloudProviders] = useState<CloudSyncProviderDescriptor[]>([]);
  const [cloudStatuses, setCloudStatuses] = useState<CloudSyncProviderStatus[]>([]);
  const [cloudFeedback, setCloudFeedback] = useState<Record<string, string>>({});
  const [busy, setBusy] = useState<string | null>(null);

  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (d) => { const updated = await api.updateSyncConfig(d); onSaved(updated.sync); },
    onError,
  );

  const loadCloud = useCallback(async () => {
    const [providers, statuses] = await Promise.all([
      api.cloudSyncProviderDescriptors(),
      api.cloudSyncProviderStatuses(),
    ]);
    setCloudProviders(providers);
    setCloudStatuses(statuses);
  }, []);

  useEffect(() => {
    loadCloud().catch((e) => onError(String(e)));
  }, [loadCloud, onError]);

  const runCloudSync = useCallback(async (provider: string) => {
    // The sync runs off the IPC worker; results arrive via the `cloud-sync-finished` listener
    // below, so we only kick it off here and let busy clear when the event lands.
    setBusy(provider);
    setCloudFeedback((current) => ({ ...current, [provider]: 'Syncing to cloud...' }));
    try {
      await api.syncManagedCloudNow(provider);
    } catch (e) {
      setCloudFeedback((current) => {
        const next = { ...current };
        delete next[provider];
        return next;
      });
      setBusy(null);
      onError(String(e));
    }
  }, [onError]);

  useEffect(() => {
    let unlistenCompleted: (() => void) | undefined;
    let unlistenFailed: (() => void) | undefined;
    let unlistenSync: (() => void) | undefined;
    let disposed = false;

    Promise.all([
      listen<CloudSyncProviderStatus>('cloud-sync://oauth-completed', (event) => {
        setCloudStatuses((prev) => upsertCloudProviderStatus(prev, event.payload));
        setCloudFeedback((current) => ({
          ...current,
          [event.payload.provider]: 'Authorization complete. Choose Use for this notebook to enable sync.',
        }));
        setBusy(null);
      }),
      listen<string>('cloud-sync://oauth-failed', (event) => {
        setBusy(null);
        onError(event.payload);
      }),
      listen<CloudSyncFinished>('cloud-sync-finished', (event) => {
        const { provider, report, error } = event.payload;
        // Only react to syncs the user kicked off from this panel (busy === provider); background
        // and note-finished syncs also emit this event but should not steal the UI.
        setBusy((current) => (current === provider ? null : current));
        if (error) {
          setCloudFeedback((current) => {
            const next = { ...current };
            delete next[provider];
            return next;
          });
          onError(error);
          return;
        }
        if (report) {
          setCloudStatuses((current) => markActiveCloudProvider(current, provider));
          setCloudFeedback((current) =>
            current[provider]
              ? { ...current, [provider]: cloudSyncReportSummary(report) }
              : current,
          );
        }
      }),
    ]).then(([completed, failed, synced]) => {
      if (disposed) {
        completed();
        failed();
        synced();
        return;
      }
      unlistenCompleted = completed;
      unlistenFailed = failed;
      unlistenSync = synced;
    }).catch((error) => onError(String(error)));

    return () => {
      disposed = true;
      unlistenCompleted?.();
      unlistenFailed?.();
      unlistenSync?.();
    };
  }, [onError]);

  const cloudStatus = (provider: string) => cloudStatuses.find((status) => status.provider === provider);

  const connectCloud = useCallback(async (provider: string) => {
    setBusy(provider);
    try {
      const start = await api.connectCloudSyncProvider(provider);
      await openUrl(start.auth_url);
    } catch (e) {
      setBusy(null);
      onError(String(e));
    }
  }, [onError]);

  const disconnectCloud = useCallback(async (provider: string) => {
    setBusy(provider);
    try {
      const disconnectedStatus = await api.disconnectCloudSyncProvider(provider);
      setCloudStatuses((current) => [
        ...current.filter((status) => status.provider !== provider),
        disconnectedStatus,
      ]);
      setCloudFeedback((current) => {
        const next = { ...current };
        delete next[provider];
        return next;
      });
    } catch (e) { onError(String(e)); }
    finally { setBusy(null); }
  }, [onError]);

  const activateCloud = useCallback(async (provider: string) => {
    setBusy(provider);
    try {
      const status = await api.activateCloudSyncProvider(provider);
      setCloudStatuses((current) => markActiveCloudProvider(upsertCloudProviderStatus(current, status), provider));
      setCloudFeedback((current) => ({
        ...current,
        [provider]: `${status.display_name} will sync this notebook.`,
      }));
    } catch (e) { onError(String(e)); }
    finally { setBusy(null); }
  }, [onError]);

  return (
    <div className="sv-subpanel">
      <Field label="Sync enabled" hint="When off, edits stay local and no CRDT sidecars are written.">
        <Toggle checked={draft.enabled} onChange={(b) => setDraft({ ...draft, enabled: b })} />
      </Field>
      <Field
        label="Merge strategy"
        hint={(
          <>
            LWW is always available; <a href={LORO_URL} target="_blank" rel="noreferrer">Loro</a> adds character-level text merge (requires the loro build feature).
          </>
        )}
      >
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
      {draft.crdt_backend !== 'loro' && (
        <p className="sv-error">
          <a href={LORO_URL} target="_blank" rel="noreferrer">Loro</a> is recommended for concurrent note edits. LWW cloud sync still works, but conflicting note bodies will not merge at character level.
        </p>
      )}
      <SaveBar saving={saving} dirty={dirty} onSave={commit} />

      <div className="sv-subhead">Cloud Sync</div>
      <p className="sv-hint">Authorize in your browser. Syllepsis stores account tokens in your operating system keychain and syncs readable notebook files to your drive.</p>
      <div className="sv-providers">
        {cloudProviders.map((provider) => {
          const status = cloudStatus(provider.provider);
          return (
            <div key={provider.provider} className="sv-provider">
              <div className="sv-provider-head">
                <span className="sv-provider-name">{provider.display_name}</span>
                <span className={`sv-pill ${status?.active_for_current_book ? 'ok' : status?.connected ? 'warn' : ''}`}>
                  {status?.active_for_current_book ? 'Active' : status?.connected ? 'Connected' : status ? 'Disconnected' : 'Keychain-backed'}
                </span>
              </div>
              <div className="sv-actions">
                <button className="sv-btn" disabled={busy === provider.provider} onClick={() => connectCloud(provider.provider)}>
                  {status?.connected ? 'Reconnect' : 'Authorize / Reauthorize'}
                </button>
                {status?.connected && !status.active_for_current_book && (
                  <button className="sv-btn" disabled={busy === provider.provider} onClick={() => activateCloud(provider.provider)}>
                    Use for this notebook
                  </button>
                )}
                <button
                  className="sv-btn"
                  disabled={dirty || saving || busy === provider.provider || !status?.active_for_current_book}
                  onClick={() => runCloudSync(provider.provider)}
                >
                  Sync now
                </button>
                <button className="sv-btn" disabled={busy === provider.provider} onClick={() => disconnectCloud(provider.provider)}>Disconnect saved</button>
              </div>
              {dirty && status?.active_for_current_book && (
                <p className="sv-connection-feedback warning">Save sync settings before cloud sync.</p>
              )}
              {status?.connected && !status.active_for_current_book && (
                <p className="sv-connection-feedback warning">Connected on this device, but not used for this notebook.</p>
              )}
              {cloudFeedback[provider.provider] && (
                <p className="sv-connection-feedback success">{cloudFeedback[provider.provider]}</p>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Device-local embedding policy ────────────────────────────────────────────────

function DeviceEmbeddingPanel({ value, onSaved, onError }: {
  value: LocalAiDevicePolicy;
  onSaved: (value: LocalAiDevicePolicy) => void;
  onError: (message: string) => void;
}) {
  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (policy) => onSaved(await api.updateLocalAiDevicePolicy(policy)),
    onError,
  );
  return (
    <div className="sv-subpanel">
      <h4 className="sv-subhead">On-device embeddings</h4>
      <p className="sv-hint">
        Synced note vectors remain usable when generation is disabled. Search queries may still
        load the model for one small inference.
      </p>
      <Field label="Generate note embeddings on this device">
        <Toggle
          checked={draft.generate_note_embeddings}
          onChange={(enabled) => setDraft({ ...draft, generate_note_embeddings: enabled })}
        />
      </Field>
      <Field label="Pause note embeddings on battery">
        <Toggle
          checked={draft.pause_note_embeddings_on_battery}
          onChange={(enabled) => setDraft({ ...draft, pause_note_embeddings_on_battery: enabled })}
        />
      </Field>
      <Field label="Embedding idle delay (seconds)">
        <NumberInput
          value={draft.note_embedding_debounce_seconds}
          onChange={(seconds) => setDraft({ ...draft, note_embedding_debounce_seconds: seconds })}
        />
      </Field>
      <Field label="Unload model after idle (seconds)">
        <NumberInput
          value={draft.model_idle_unload_seconds}
          onChange={(seconds) => setDraft({ ...draft, model_idle_unload_seconds: seconds })}
        />
      </Field>
      <SaveBar saving={saving} dirty={dirty} onSave={commit} />
    </div>
  );
}

// ── Advanced: embedding model ────────────────────────────────────────────────────

function EmbeddingPanel({ value, models, onSaved, onError }: {
  value: EmbeddingConfig;
  models: ModelManifest[];
  onSaved: (value: EmbeddingConfig) => void;
  onError: (message: string) => void;
}) {
  const { draft, setDraft, dirty, saving, commit } = useSectionDraft(
    value,
    async (embedding) => {
      const updated = await api.updateEmbeddingConfig(embedding);
      onSaved(updated.embedding);
    },
    onError,
  );
  return (
    <div className="sv-subpanel">
      <h4 className="sv-subhead">Embedding model</h4>
      <Field
        label="Model"
        hint="Changing models invalidates existing vectors. Only fully supported embedding manifests are listed."
      >
        <select
          className="sv-select"
          value={draft.model_id}
          onChange={(event) => setDraft({ ...draft, model_id: event.target.value })}
        >
          {models.map((model) => (
            <option key={model.id} value={model.id}>{model.display_name}</option>
          ))}
        </select>
      </Field>
      <Field label="MRL dimensions">
        <NumberInput
          value={draft.matryoshka_dims ?? 256}
          onChange={(dimensions) => setDraft({ ...draft, matryoshka_dims: dimensions })}
        />
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

// ── Plugins ─────────────────────────────────────────────────────────────────────

function PluginsPanel({ plugins, onPluginInstalled, onPluginsChanged, onError }: {
  plugins: PluginDescriptor[];
  onPluginInstalled: (name: string) => void;
  onPluginsChanged: () => void;
  onError: (m: string) => void;
}) {
  const [installing, setInstalling] = useState(false);
  const [togglingId, setTogglingId] = useState<string | null>(null);

  const addPlugin = useCallback(async () => {
    const selected = await openDialog({
      multiple: false,
      title: 'Select plugin file',
      filters: [{ name: 'WASM plugin', extensions: ['wasm'] }],
    });
    if (!selected || typeof selected !== 'string') return;
    setInstalling(true);
    try {
      const name = await api.installUserPlugin(selected);
      onPluginInstalled(name);
    } catch (e) {
      onError(String(e));
    } finally {
      setInstalling(false);
    }
  }, [onPluginInstalled, onError]);

  const togglePlugin = useCallback(async (plugin: PluginDescriptor) => {
    setTogglingId(plugin.id);
    try {
      await api.setPluginEnabled(plugin.id, !plugin.enabled);
      onPluginsChanged();
    } catch (e) {
      onError(String(e));
    } finally {
      setTogglingId(null);
    }
  }, [onPluginsChanged, onError]);

  return (
    <div className="sv-subpanel">
      {plugins.length === 0 ? (
        <div className="sv-plugins">
          <Icon name="extension" size={22} className="sv-plugins-icon" />
          <div>
            <div className="sv-plugins-title">No plugins installed</div>
            <p className="sv-plugins-text">Sandboxed (WASM) extensions add import sources and code-block renderers. Built-in plugins load automatically; install your own with the button below.</p>
          </div>
        </div>
      ) : (
        <div className="sv-plugin-list">
          {plugins.map((plugin) => (
            <div key={plugin.id} className={`sv-plugin-item ${plugin.enabled ? '' : 'sv-plugin-item--disabled'}`}>
              <Icon name={plugin.kind === 'import_source' ? 'upload_file' : 'code'} size={20} className="sv-plugins-icon" />
              <div className="sv-plugin-body">
                <div className="sv-plugin-head">
                  <span className="sv-plugins-title">{plugin.name}</span>
                  <span className="sv-plugin-version">v{plugin.version}</span>
                  <span className="sv-plugin-badge">{plugin.source === 'builtin' ? 'Built-in' : 'User'}</span>
                  <span className="sv-plugin-badge">{plugin.kind === 'import_source' ? 'Import source' : 'Code block'}</span>
                  {!plugin.enabled && <span className="sv-plugin-badge sv-plugin-badge--off">Disabled</span>}
                </div>
                {plugin.description && <p className="sv-plugins-text">{plugin.description}</p>}
                {plugin.kind === 'code_block_renderer' && plugin.languages.length > 0 && (
                  <p className="sv-plugins-text">Languages: {plugin.languages.join(', ')}</p>
                )}
                {plugin.kind === 'import_source' && plugin.import_extensions.length > 0 && (
                  <p className="sv-plugins-text">Files: .{plugin.import_extensions.join(', .')}</p>
                )}
              </div>
              <div className="sv-plugin-actions">
                <Toggle
                  checked={plugin.enabled}
                  onChange={() => togglingId === null && togglePlugin(plugin)}
                />
              </div>
            </div>
          ))}
        </div>
      )}
      <div className="sv-savebar">
        <button className="sv-btn sv-btn-primary" onClick={addPlugin} disabled={installing}>
          {installing ? 'Installing…' : 'Add plugin…'}
        </button>
      </div>
    </div>
  );
}

function DeleteBookPanel({
  bookName,
  onDeleted,
  onError,
}: {
  bookName: string;
  onDeleted: (report: DeleteCurrentBookReport) => void;
  onError: (message: string) => void;
}) {
  const [confirmation, setConfirmation] = useState('');
  const [deleting, setDeleting] = useState(false);
  const canDelete = confirmation === bookName;

  const deleteBook = useCallback(async () => {
    if (!canDelete) return;
    setDeleting(true);
    try {
      const report = await api.deleteCurrentBook(bookName);
      onDeleted(report);
    } catch (e) {
      onError(String(e));
    } finally {
      setDeleting(false);
    }
  }, [bookName, canDelete, onDeleted, onError]);

  return (
    <div className="sv-delete-book">
      <p className="sv-delete-book-warning">
        This permanently deletes the notebook folder from disk and removes it from the launcher.
        Managed cloud data is cleaned up for connected providers associated with this notebook.
      </p>
      <Field
        label="Type notebook name to confirm"
        hint={`Type exactly: ${bookName}`}
      >
        <input
          className="sv-input sv-delete-book-input"
          value={confirmation}
          onChange={(event) => setConfirmation(event.target.value)}
          placeholder={bookName}
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
        />
      </Field>
      <div className="sv-savebar">
        <button
          className="sv-btn sv-btn-danger"
          onClick={deleteBook}
          disabled={!canDelete || deleting}
        >
          {deleting ? 'Deleting…' : 'Delete notebook'}
        </button>
      </div>
    </div>
  );
}

// ── Local Search API ──────────────────────────────────────────────────────────

function SearchApiPanel({
  status,
  onChanged,
  onError,
}: {
  status: SearchApiStatus;
  onChanged: (s: SearchApiStatus) => void;
  onError: (m: string) => void;
}) {
  const [busy, setBusy] = useState(false);

  const toggle = useCallback(async (enabled: boolean) => {
    setBusy(true);
    try {
      const updated = await api.setSearchApiEnabled(enabled);
      onChanged(updated);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [onChanged, onError]);

  const regenerate = useCallback(async () => {
    setBusy(true);
    try {
      const updated = await api.regenerateSearchApiToken();
      onChanged(updated);
    } catch (e) {
      onError(String(e));
    } finally {
      setBusy(false);
    }
  }, [onChanged, onError]);

  const copy = useCallback(async (text: string) => {
    await navigator.clipboard?.writeText(text).catch(() => undefined);
  }, []);

  const mcpConfig = status.enabled && status.token
    ? JSON.stringify(
        {
          mcpServers: {
            syllepsis: {
              url: status.mcp_url,
              headers: { Authorization: `Bearer ${status.token}` },
            },
          },
        },
        null,
        2,
      )
    : '';

  return (
    <div className="sv-subpanel">
      <p className="sv-hint">
        Exposes Syllepsis search over a localhost-only HTTP server. Read-only. Bind address: 127.0.0.1:{status.port}.
      </p>
      <Field label="Enable API" hint="Starts the server on the port below. Off by default.">
        <Toggle checked={status.enabled} onChange={(v) => { if (!busy) toggle(v); }} />
      </Field>

      {status.enabled && status.token && (
        <>
          <Field label="Bearer token" hint="Required on every request. Treat like a password.">
            <div className="sv-token-row">
              <code className="sv-token">{status.token}</code>
              <button className="sv-btn sv-btn-compact" type="button" onClick={() => copy(status.token!)}>Copy</button>
              <button className="sv-btn sv-btn-compact" type="button" disabled={busy} onClick={regenerate}>Rotate</button>
            </div>
          </Field>

          <Field label="REST base URL" hint="All /api/* routes require the bearer token.">
            <div className="sv-token-row">
              <code className="sv-token">{status.rest_url}</code>
              <button className="sv-btn sv-btn-compact" type="button" onClick={() => copy(status.rest_url)}>Copy</button>
            </div>
          </Field>

          <Field label="MCP endpoint" hint="JSON-RPC 2.0 — tools: search, get_note, recent_notes, core_notes, notes_by_category.">
            <div className="sv-token-row">
              <code className="sv-token">{status.mcp_url}</code>
              <button className="sv-btn sv-btn-compact" type="button" onClick={() => copy(status.mcp_url)}>Copy</button>
            </div>
          </Field>

          <Field label="MCP client config" hint="Paste into your MCP client (Claude Desktop, Cursor, etc.).">
            <div className="sv-token-row">
              <code className="sv-token sv-token-pre">{mcpConfig}</code>
              <button className="sv-btn sv-btn-compact" type="button" onClick={() => copy(mcpConfig)}>Copy</button>
            </div>
          </Field>
        </>
      )}
    </div>
  );
}
