// Statistics & analytics dashboard: note counts, category usage, and other book health metrics.

import { useCallback, useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { PageHeader } from '../components/PageHeader';
import type { BookStats, LocalAiStatus, LocalAiDevicePolicy, OperationalActivitySummary } from '../types';
import './StatsView.css';

type StatsTab = 'overview' | 'local-ai' | 'activity' | 'distribution';

const TAB_LABELS: Record<StatsTab, string> = {
  'overview': 'Overview',
  'local-ai': 'Local AI',
  'activity': 'Activity & Sync',
  'distribution': 'Distribution',
};

function StatCard({ label, value, sub }: { label: string; value: string | number; sub?: string }) {
  return (
    <div className="stats-card">
      <div className="stats-card-value">{value}</div>
      <div className="stats-card-label">{label}</div>
      {sub && <div className="stats-card-sub">{sub}</div>}
    </div>
  );
}

function SummaryRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="stats-summary-row">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function formatJobLabel(raw: string | null): string {
  if (!raw) return 'Idle';
  if (raw.startsWith('embedding:note:')) return 'Embedding a note';
  if (raw === 'embedding:search-query') return 'Running a search';
  if (raw.startsWith('llm:')) return 'Generating with the language model';
  return raw;
}

export function StatsView() {
  const [tab, setTab] = useState<StatsTab>('overview');
  const [stats, setStats] = useState<BookStats | null>(null);
  const [operational, setOperational] = useState<OperationalActivitySummary | null>(null);
  const [localAi, setLocalAi] = useState<LocalAiStatus | null>(null);
  const [policy, setPolicy] = useState<LocalAiDevicePolicy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [bookStats, operationalSummary, localAiStatus, devicePolicy] = await Promise.all([
        api.bookStats(),
        api.operationalActivitySummary(),
        api.localAiStatus(),
        api.getLocalAiDevicePolicy(),
      ]);
      setStats(bookStats);
      setOperational(operationalSummary);
      setLocalAi(localAiStatus);
      setPolicy(devicePolicy);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  const togglePolicy = useCallback(async (
    key: 'generate_note_embeddings' | 'pause_note_embeddings_on_battery',
    value: boolean,
  ) => {
    if (!policy) return;
    const prev = policy;
    const updated = { ...policy, [key]: value };
    setPolicy(updated);
    try {
      await api.updateLocalAiDevicePolicy(updated);
    } catch {
      setPolicy(prev);
    }
  }, [policy]);

  const typeEntries = useMemo(
    () => Object.entries(stats?.notes_by_type ?? {}).sort((a, b) => b[1] - a[1]),
    [stats],
  );
  const categoryEntries = useMemo(
    () => Object.entries(stats?.notes_by_category ?? {}).sort((a, b) => b[1] - a[1]),
    [stats],
  );

  if (loading) return <div className="stats-state">Computing stats…</div>;
  if (error) return <div className="stats-state stats-error">{error}</div>;
  if (!stats) return null;

  const sortedPercent = stats.total_notes > 0
    ? Math.round((stats.sorted_notes / stats.total_notes) * 100)
    : 0;


  return (
    <div className="stats-root">
      <PageHeader
        title="Book Statistics"
        secondary={
          <div className="stats-tabs">
            {(Object.keys(TAB_LABELS) as StatsTab[]).map((t) => (
              <button
                key={t}
                className={`stats-tab${tab === t ? ' active' : ''}`}
                onClick={() => setTab(t)}
              >
                {TAB_LABELS[t]}
              </button>
            ))}
          </div>
        }
      >
        <button className="stats-refresh-btn" onClick={load}>Refresh</button>
      </PageHeader>

      <div className="stats-body">
      {tab === 'overview' && (
        <section className="stats-section">
          <h3 className="stats-section-title">Overview</h3>
          <div className="stats-grid">
            <StatCard label="Total notes" value={stats.total_notes} />
            <StatCard label="Sorted" value={stats.sorted_notes} sub={`${sortedPercent}% of total`} />
            <StatCard label="Unsorted" value={stats.unsorted_notes} sub="in inbox" />
            <StatCard label="Categories" value={stats.total_categories} />
            <StatCard label="Starred" value={stats.starred_notes} />
            <StatCard label="Hidden" value={stats.hidden_notes} />
            <StatCard label="Archived" value={stats.archived_notes} />
            <StatCard label="With location" value={stats.notes_with_location} />
            <StatCard label="Avg word count" value={stats.avg_word_count} sub="per note" />
            <StatCard label="With attachments" value={stats.notes_with_attachments} />
            <StatCard label="AI-generated" value={stats.ai_generated_notes} />
            <StatCard label="Uncategorized" value={stats.uncategorized_notes} sub="no tags" />
            <StatCard label="Created this week" value={stats.created_this_week} sub="last 7 days" />
            <StatCard label="Updated this week" value={stats.updated_this_week} sub="last 7 days" />
            <StatCard label="Overdue tasks" value={stats.overdue_tasks} sub="past due, not done" />
            <StatCard label="Core priority" value={stats.core_priority_notes} />
            <StatCard label="Scheduled for deletion" value={stats.scheduled_for_deletion} sub="in trash, pending purge" />
          </div>
        </section>
      )}

      {tab === 'local-ai' && localAi && (
        <LocalAiSection
          localAi={localAi}
          policy={policy}
          onTogglePolicy={togglePolicy}
          onAction={async () => {
            if (!localAi.embedding_model_cached) {
              await api.downloadBuiltinModel(localAi.embedding_model_id);
            }
            await api.enqueueAllStaleEmbeddings();
            await load();
          }}
        />
      )}

      {tab === 'activity' && operational && (
        <>
          <section className="stats-section">
            <h3 className="stats-section-title">Operational activity</h3>
            <div className="stats-grid">
              <StatCard
                label="External updates"
                value={operational.activity.external_updates_24h}
                sub={`${operational.activity.external_updates_7d} unique files in 7 days`}
              />
              <StatCard
                label="Updated notes"
                value={operational.activity.external_note_updates_24h}
                sub="external updates in 24 hours"
              />
              <StatCard
                label="Remote Loro merges"
                value={operational.activity.remote_loro_merges_7d}
                sub="last 7 days"
              />
              <StatCard
                label="Conflict copies"
                value={operational.activity.conflict_copies_7d}
                sub="last 7 days"
              />
            </div>
            <div className="stats-summary-panel">
              <SummaryRow
                label="Latest external update"
                value={formatRelativeTime(operational.activity.latest_external_update_at)}
              />
              <SummaryRow
                label="Latest remote Loro merge"
                value={formatRelativeTime(operational.activity.latest_remote_loro_merge_at)}
              />
              <SummaryRow
                label="Latest conflict path"
                value={operational.activity.latest_conflict_path ?? 'None'}
              />
            </div>
          </section>

          <section className="stats-section">
            <h3 className="stats-section-title">Repository and sync health</h3>
            <div className="stats-grid">
              <StatCard
                label="Git changes"
                value={operational.git.changed_file_count}
                sub={
                  operational.git.is_repository
                    ? `${operational.git.commit_safe_note_change_count} note files commit-ready`
                    : 'not a git repository'
                }
              />
              <StatCard
                label="Git branch"
                value={operational.git.branch ?? '—'}
                sub={operational.git.available ? 'repository status' : 'git unavailable'}
              />
              <StatCard
                label="Cloud providers"
                value={`${operational.cloud.connected_provider_count}`}
                sub={
                  operational.cloud.connected_provider_names.length > 0
                    ? operational.cloud.connected_provider_names.join(', ')
                    : 'none connected'
                }
              />
              <StatCard
                label="CRDT sidecars"
                value={`${operational.crdt.loro_sidecar_coverage_percent}%`}
                sub={`${operational.crdt.sidecar_count}/${operational.crdt.note_count} notes, ${operational.crdt.backend}`}
              />
            </div>
            {(operational.git.error || operational.cloud.error) && (
              <div className="stats-summary-panel">
                {operational.git.error && <SummaryRow label="Git note" value={operational.git.error} />}
                {operational.cloud.error && <SummaryRow label="Cloud note" value={operational.cloud.error} />}
              </div>
            )}
          </section>
        </>
      )}

      {tab === 'distribution' && (
        <>
          {typeEntries.length > 0 && (
            <section className="stats-section">
              <h3 className="stats-section-title">Notes by type</h3>
              <div className="stats-bar-list">
                {typeEntries.map(([type, count]) => (
                  <div key={type} className="stats-bar-row">
                    <span className="stats-bar-label">{type}</span>
                    <div className="stats-bar-track">
                      <div
                        className="stats-bar-fill"
                        style={{ width: `${(count / stats.total_notes) * 100}%` }}
                      />
                    </div>
                    <span className="stats-bar-count">{count}</span>
                  </div>
                ))}
              </div>
            </section>
          )}

          {categoryEntries.length > 0 && (
            <section className="stats-section">
              <h3 className="stats-section-title">Notes per category</h3>
              <div className="stats-bar-list">
                {categoryEntries.slice(0, 20).map(([cat, count]) => {
                  const max = categoryEntries[0][1];
                  return (
                    <div key={cat} className="stats-bar-row">
                      <span className="stats-bar-label">#{cat}</span>
                      <div className="stats-bar-track">
                        <div
                          className="stats-bar-fill stats-bar-fill-secondary"
                          style={{ width: `${(count / max) * 100}%` }}
                        />
                      </div>
                      <span className="stats-bar-count">{count}</span>
                    </div>
                  );
                })}
                {categoryEntries.length > 20 && (
                  <p className="stats-hint">…and {categoryEntries.length - 20} more categories</p>
                )}
              </div>
            </section>
          )}
        </>
      )}
      </div>
    </div>
  );
}

interface LocalAiSectionProps {
  localAi: LocalAiStatus;
  policy: LocalAiDevicePolicy | null;
  onTogglePolicy: (key: 'generate_note_embeddings' | 'pause_note_embeddings_on_battery', value: boolean) => void;
  onAction: () => Promise<void>;
}

function LocalAiSection({ localAi, policy, onTogglePolicy, onAction }: LocalAiSectionProps) {
  const [busy, setBusy] = useState(false);
  const cov = localAi.embedding_coverage;
  const total = cov.total_notes || 1;

  const segments = [
    { key: 'fresh', label: 'Fresh', count: cov.fresh_notes, color: 'var(--color-accent)' },
    { key: 'stale', label: 'Stale', count: cov.stale_notes, color: 'var(--color-secondary)' },
    { key: 'missing', label: 'Missing', count: cov.missing_notes, color: 'var(--color-text-tertiary)' },
    { key: 'incompatible', label: 'Incompatible', count: cov.incompatible_notes, color: 'var(--color-border)' },
    { key: 'blocked', label: 'Blocked', count: cov.blocked_notes, color: 'var(--color-error)' },
  ].filter((s) => s.count > 0);

  const friendlyJob = formatJobLabel(localAi.worker.current_job);
  const rawJob = localAi.worker.current_job ?? 'idle';
  const isIdle = !localAi.worker.current_job;

  return (
    <section className="stats-section">
      <h3 className="stats-section-title">Local AI</h3>

      <div className={`stats-job-banner${isIdle ? ' stats-job-banner-idle' : ''}`} title={rawJob}>
        <span className="stats-job-label">{friendlyJob}</span>
        <span className="stats-job-sub">
          {localAi.worker.power_source === 'battery' ? 'on battery' : 'on AC'}
        </span>
      </div>

      <div className="stats-cov-block">
        <div className="stats-cov-header">
          <span className="stats-section-sublabel">Embedding coverage</span>
          <span className="stats-cov-count">{cov.fresh_notes} / {cov.total_notes} fresh</span>
        </div>
        {cov.total_notes > 0 ? (
          <>
            <div className="stats-seg-bar">
              {segments.map((s) => (
                <div
                  key={s.key}
                  className="stats-seg-bar-segment"
                  style={{ width: `${(s.count / total) * 100}%`, background: s.color }}
                  title={`${s.label}: ${s.count}`}
                />
              ))}
            </div>
            <div className="stats-seg-legend">
              {segments.map((s) => (
                <span key={s.key} className="stats-seg-legend-item">
                  <span className="stats-seg-dot" style={{ background: s.color }} />
                  {s.label} ({s.count})
                </span>
              ))}
            </div>
          </>
        ) : (
          <p className="stats-hint">No notes in this book yet.</p>
        )}
      </div>

      <div className="stats-summary-panel">
        <SummaryRow
          label="Model"
          value={`${localAi.embedding_model_id} ${localAi.embedding_model_cached ? '(cached)' : '(not downloaded)'}`}
        />
        <SummaryRow
          label="Queued"
          value={`${localAi.worker.pending_note_jobs} note${localAi.worker.pending_note_jobs !== 1 ? 's' : ''}${localAi.worker.blocked_note_jobs > 0 ? ` (${localAi.worker.blocked_note_jobs} blocked)` : ''}`}
        />
        {(localAi.worker.pending_llm_jobs > 0 || localAi.worker.pending_query_jobs > 0) && (
          <SummaryRow
            label="Interactive queue"
            value={`${localAi.worker.pending_llm_jobs} LLM, ${localAi.worker.pending_query_jobs} search`}
          />
        )}
        {localAi.worker.note_block_reason && (
          <SummaryRow label="Block reason" value={localAi.worker.note_block_reason} />
        )}
        {localAi.worker.recent_failures[0] && (
          <SummaryRow label="Latest failure" value={localAi.worker.recent_failures[0].message} />
        )}
      </div>

      {policy && (
        <div className="stats-policy-block">
          <label className="stats-policy-toggle">
            <input
              type="checkbox"
              checked={policy.generate_note_embeddings}
              onChange={(e) => onTogglePolicy('generate_note_embeddings', e.target.checked)}
            />
            <span>Generate note embeddings on this device</span>
          </label>
          <label className="stats-policy-toggle">
            <input
              type="checkbox"
              checked={policy.pause_note_embeddings_on_battery}
              onChange={(e) => onTogglePolicy('pause_note_embeddings_on_battery', e.target.checked)}
            />
            <span>Pause embedding generation on battery</span>
          </label>
        </div>
      )}

      <button
        className="stats-refresh-btn"
        disabled={busy}
        onClick={async () => {
          setBusy(true);
          try { await onAction(); } finally { setBusy(false); }
        }}
      >
        {localAi.embedding_model_cached ? 'Queue stale embeddings' : 'Download model and resume'}
      </button>
    </section>
  );
}

function formatRelativeTime(value?: string) {
  if (!value) return 'None';
  const timestamp = new Date(value).getTime();
  if (Number.isNaN(timestamp)) return value;
  const seconds = Math.max(0, Math.round((Date.now() - timestamp) / 1000));
  if (seconds < 60) return 'just now';
  const minutes = Math.round(seconds / 60);
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}
