// Statistics & analytics dashboard: note counts, category usage, and other book health metrics.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import type { BookStats, OperationalActivitySummary } from '../types';
import './StatsView.css';

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

export function StatsView() {
  const [stats, setStats] = useState<BookStats | null>(null);
  const [operational, setOperational] = useState<OperationalActivitySummary | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [bookStats, operationalSummary] = await Promise.all([
        api.bookStats(),
        api.operationalActivitySummary(),
      ]);
      setStats(bookStats);
      setOperational(operationalSummary);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  if (loading) return <div className="stats-state">Computing stats…</div>;
  if (error) return <div className="stats-state stats-error">{error}</div>;
  if (!stats) return null;

  const sortedPercent = stats.total_notes > 0
    ? Math.round((stats.sorted_notes / stats.total_notes) * 100)
    : 0;

  const typeEntries = Object.entries(stats.notes_by_type).sort((a, b) => b[1] - a[1]);
  const categoryEntries = Object.entries(stats.notes_by_category).sort((a, b) => b[1] - a[1]);

  return (
    <div className="stats-root">
      <div className="stats-header">
        <h2 className="stats-title">Book Statistics</h2>
        <button className="stats-refresh-btn" onClick={load}>Refresh</button>
      </div>

      <section className="stats-section">
        <h3 className="stats-section-title">Overview</h3>
        <div className="stats-grid">
          <StatCard label="Total notes" value={stats.total_notes} />
          <StatCard label="Sorted" value={stats.sorted_notes} sub={`${sortedPercent}% of total`} />
          <StatCard label="Unsorted" value={stats.unsorted_notes} sub="in inbox" />
          <StatCard label="Categories" value={stats.total_categories} />
          <StatCard label="Starred" value={stats.starred_notes} />
          <StatCard label="Private" value={stats.private_notes} />
          <StatCard label="Archived" value={stats.archived_notes} />
          <StatCard label="With location" value={stats.notes_with_location} />
        </div>
      </section>

      {operational && (
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
      )}

      {operational && (
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
      )}

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
    </div>
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
