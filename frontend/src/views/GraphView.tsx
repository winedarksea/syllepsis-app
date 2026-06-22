// A lightweight, dependency-free graph of the book: each note is a node clustered by its
// first category; prior relationships are drawn as edges. Deterministic layout (category
// clusters on a ring, notes on a small circle within each) keeps it stable without a physics
// engine. Click a node to open it.

import { useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import type { NoteDto } from '../types';
import './GraphView.css';

const WIDTH = 1000;
const HEIGHT = 720;
const PALETTE = ['#4f8cff', '#34c98b', '#e0a23a', '#d9647a', '#9b6bdf', '#46b8c8', '#c97b4f', '#7a8aa0'];

interface Node {
  id: string;
  title: string;
  x: number;
  y: number;
  color: string;
}

function buildLayout(notes: NoteDto[]): { nodes: Node[]; edges: [string, string][] } {
  const groups = new Map<string, NoteDto[]>();
  for (const n of notes) {
    const key = n.categories[0] ?? '·uncategorized';
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(n);
  }

  const groupKeys = [...groups.keys()];
  const cx = WIDTH / 2;
  const cy = HEIGHT / 2;
  const ringRadius = Math.min(WIDTH, HEIGHT) * 0.34;
  const nodes: Node[] = [];

  groupKeys.forEach((key, gi) => {
    const groupAngle = (gi / Math.max(groupKeys.length, 1)) * Math.PI * 2;
    const groupX = groupKeys.length === 1 ? cx : cx + Math.cos(groupAngle) * ringRadius;
    const groupY = groupKeys.length === 1 ? cy : cy + Math.sin(groupAngle) * ringRadius;
    const members = groups.get(key)!;
    const localRadius = Math.min(120, 30 + members.length * 8);
    const color = PALETTE[gi % PALETTE.length];

    members.forEach((n, mi) => {
      const a = (mi / Math.max(members.length, 1)) * Math.PI * 2;
      const r = members.length === 1 ? 0 : localRadius;
      nodes.push({
        id: n.id,
        title: n.title || '(untitled)',
        x: groupX + Math.cos(a) * r,
        y: groupY + Math.sin(a) * r,
        color,
      });
    });
  });

  const present = new Set(nodes.map((n) => n.id));
  const edges: [string, string][] = [];
  for (const n of notes) {
    const target = n.prior?.target;
    if (target && 'note' in target && present.has(target.note)) {
      edges.push([n.id, target.note]);
    }
  }

  return { nodes, edges };
}

export function GraphView() {
  const { openEditor } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);

  useEffect(() => {
    api.listNotes().then(setNotes).catch((e) => setError(String(e)));
  }, []);

  const { nodes, edges } = useMemo(() => buildLayout(notes), [notes]);
  const byId = useMemo(() => new Map(nodes.map((n) => [n.id, n])), [nodes]);

  if (error) return <div className="gv-state gv-error">{error}</div>;
  if (notes.length === 0) return <div className="gv-state">No notes to graph yet.</div>;

  return (
    <div className="gv-root">
      <div className="gv-header"><h2 className="gv-title">Graph</h2></div>
      <div className="gv-canvas">
        <svg viewBox={`0 0 ${WIDTH} ${HEIGHT}`} className="gv-svg">
          <g className="gv-edges">
            {edges.map(([from, to], i) => {
              const a = byId.get(from);
              const b = byId.get(to);
              if (!a || !b) return null;
              return <line key={i} x1={a.x} y1={a.y} x2={b.x} y2={b.y} className="gv-edge" />;
            })}
          </g>
          <g className="gv-nodes">
            {nodes.map((n) => (
              <g
                key={n.id}
                transform={`translate(${n.x} ${n.y})`}
                className="gv-node"
                onClick={() => openEditor(n.id)}
                onMouseEnter={() => setHover(n.id)}
                onMouseLeave={() => setHover((h) => (h === n.id ? null : h))}
              >
                <circle r={hover === n.id ? 9 : 6} fill={n.color} className="gv-node-dot" />
                {hover === n.id && (
                  <text x={11} y={4} className="gv-node-label">{n.title}</text>
                )}
              </g>
            ))}
          </g>
        </svg>
      </div>
    </div>
  );
}
