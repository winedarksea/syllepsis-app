// Statistics & analytics dashboard: note counts, category usage, and other book health metrics.

import { useCallback, useEffect, useState } from 'react';
import { api } from '../lib/api';
import type { BookStats } from '../types';
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

export function StatsView() {
  const [stats, setStats] = useState<BookStats | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      setStats(await api.bookStats());
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
