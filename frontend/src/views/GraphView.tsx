// A lightweight, dependency-free graph of the book: each note is a node clustered by its
// first category; prior relationships are drawn as edges. Deterministic layout (category
// clusters on a ring, notes on a small circle within each) keeps it stable without a physics
// engine. Click a node to open it.

import { useEffect, useMemo, useState } from 'react';
import { api } from '../lib/api';
import { useStore } from '../lib/store';
import { useThemeStyle } from '../components/Icon';
import type { NoteDto } from '../types';
import './GraphView.css';

const WIDTH = 1000;
const HEIGHT = 720;
// Above this edge count, drop the weave casing to keep dense graphs legible.
const WEAVE_LIMIT = 140;
// Cluster colors come from theme tokens (see .gv-node--N in GraphView.css);
// we only carry the cluster index so the palette stays theme-driven.
const CLUSTERS = 5;

interface Node {
  id: string;
  title: string;
  x: number;
  y: number;
  cluster: number;
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
    const cluster = gi % CLUSTERS;

    members.forEach((n, mi) => {
      const a = (mi / Math.max(members.length, 1)) * Math.PI * 2;
      const r = members.length === 1 ? 0 : localRadius;
      nodes.push({
        id: n.id,
        title: n.title || '(untitled)',
        x: groupX + Math.cos(a) * r,
        y: groupY + Math.sin(a) * r,
        cluster,
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

// Returns the SVG shape element for a graph node based on the active theme style.
function NodeShape({ graphNode, hovered }: { graphNode: string; hovered: boolean }) {
  const r = hovered ? 8 : 6;
  if (graphNode === 'star') {
    // 4-point star / point-of-light
    const s = r * 0.42;
    const o = r;
    const d = `M0,${-o} L${s},${-s} L${o},0 L${s},${s} L0,${o} L${-s},${s} L${-o},0 L${-s},${-s} Z`;
    return <path d={d} className="gv-node-dot" />;
  }
  if (graphNode === 'hex') {
    // Flat-topped hexagon
    const pts = Array.from({ length: 6 }, (_, i) => {
      const a = (Math.PI / 3) * i;
      return `${(r * Math.cos(a)).toFixed(2)},${(r * Math.sin(a)).toFixed(2)}`;
    }).join(' ');
    return <polygon points={pts} className="gv-node-dot" />;
  }
  // disc (default)
  return <circle r={r} className="gv-node-dot" />;
}

export function GraphView() {
  const { openEditor } = useStore();
  const [notes, setNotes] = useState<NoteDto[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<string | null>(null);
  const themeStyle = useThemeStyle();

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
          {/* Edges: weave (casing+line), glow (drop-shadow via CSS), or plain (flat stroke). */}
          <g className="gv-edges">
            {edges.map(([from, to], i) => {
              const a = byId.get(from);
              const b = byId.get(to);
              if (!a || !b) return null;
              const showCasing = themeStyle.graphEdge === 'weave' && edges.length <= WEAVE_LIMIT;
              return (
                <g key={i}>
                  {showCasing && (
                    <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} className="gv-edge-casing" />
                  )}
                  <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} className="gv-edge" />
                </g>
              );
            })}
          </g>
          <g className="gv-nodes">
            {nodes.map((n) => (
              <g
                key={n.id}
                transform={`translate(${n.x} ${n.y})`}
                className={`gv-node gv-node--${n.cluster}${hover === n.id ? ' gv-node-active' : ''}`}
                onClick={() => openEditor(n.id)}
                onMouseEnter={() => setHover(n.id)}
                onMouseLeave={() => setHover((h) => (h === n.id ? null : h))}
              >
                <NodeShape graphNode={themeStyle.graphNode} hovered={hover === n.id} />
                {hover === n.id && (
                  <text x={12} y={4} className="gv-node-label">{n.title}</text>
                )}
              </g>
            ))}
          </g>
        </svg>
      </div>
    </div>
  );
}
