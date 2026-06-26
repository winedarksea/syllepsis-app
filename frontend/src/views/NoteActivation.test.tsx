import { cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { GraphCanvas } from './GraphCanvas';
import { TimelineCanvas } from './TimelineCanvas';
import { WorldStage } from './WorldStage';
import type { GraphAnalysisResult, Overlay } from '../types';

const graphSvgRect = {
  x: 0,
  y: 0,
  left: 0,
  top: 0,
  right: 1200,
  bottom: 720,
  width: 1200,
  height: 720,
  toJSON: () => ({}),
} as DOMRect;

const worldSvgRect = {
  ...graphSvgRect,
  bottom: 620,
  height: 620,
} as DOMRect;

let worldTranslateY = 0;

beforeEach(() => {
  worldTranslateY = 0;
  Object.defineProperty(SVGElement.prototype, 'getBoundingClientRect', {
    configurable: true,
    value() {
      return this.classList?.contains('wv-svg-stage') ? worldSvgRect : graphSvgRect;
    },
  });
  Object.defineProperty(SVGElement.prototype, 'getScreenCTM', {
    configurable: true,
    value() {
      if (this.classList?.contains('wv-svg-stage')) return svgMatrix(1, 0, worldTranslateY);
      return svgMatrix(1, 100, 0);
    },
  });
  Object.defineProperty(SVGElement.prototype, 'createSVGPoint', {
    configurable: true,
    value: () => ({
      x: 0,
      y: 0,
      matrixTransform(this: { x: number; y: number }, matrix: { a: number; d: number; e: number; f: number }) {
        return {
          x: this.x * matrix.a + matrix.e,
          y: this.y * matrix.d + matrix.f,
        };
      },
    }),
  });
  Object.defineProperty(SVGElement.prototype, 'setPointerCapture', {
    configurable: true,
    value: vi.fn(),
  });
  Object.defineProperty(SVGElement.prototype, 'releasePointerCapture', {
    configurable: true,
    value: vi.fn(),
  });
  Object.defineProperty(SVGElement.prototype, 'hasPointerCapture', {
    configurable: true,
    value: () => true,
  });
});

afterEach(() => {
  cleanup();
});

describe('note activation in SVG views', () => {
  it('opens graph nodes on pointer click', () => {
    const onOpenNote = vi.fn();
    const { container } = render(
      <GraphCanvas
        result={graphResult()}
        semanticEdges={[]}
        showAllTitles={false}
        loading={false}
        onOpenNote={onOpenNote}
      />,
    );
    const svg = container.querySelector('svg')!;

    fireEvent.pointerDown(svg, { pointerId: 1, clientX: 600, clientY: 360 });
    fireEvent.pointerUp(svg, { pointerId: 1, clientX: 600, clientY: 360 });

    expect(onOpenNote).toHaveBeenCalledWith('note-1');
  });

  it('opens graph nodes when the SVG drawing is letterboxed', () => {
    const onOpenNote = vi.fn();
    const { container } = render(
      <GraphCanvas
        result={graphResult({ x: 0, y: 0.5 })}
        semanticEdges={[]}
        showAllTitles={false}
        loading={false}
        onOpenNote={onOpenNote}
      />,
    );
    const svg = container.querySelector('svg')!;

    fireEvent.pointerDown(svg, { pointerId: 1, clientX: 170, clientY: 360 });
    fireEvent.pointerUp(svg, { pointerId: 1, clientX: 170, clientY: 360 });

    expect(onOpenNote).toHaveBeenCalledWith('note-1');
  });

  it('opens timeline nodes on pointer click', () => {
    const onOpenNote = vi.fn();
    const { container } = render(
      <TimelineCanvas
        result={graphResult({ mode: 'timeline', includeTimeline: true })}
        showAllTitles={false}
        showPriorRelationships={false}
        colorBy="category"
        loading={false}
        onOpenNote={onOpenNote}
      />,
    );
    const svg = container.querySelector('svg')!;

    fireEvent.pointerDown(svg, { pointerId: 1, clientX: 600, clientY: 319 });
    fireEvent.pointerUp(svg, { pointerId: 1, clientX: 600, clientY: 319 });

    expect(onOpenNote).toHaveBeenCalledWith('note-1');
  });

  it('opens world note pins and category regions without opening during drag', () => {
    const onOpenNote = vi.fn();
    const onOpenCategory = vi.fn();
    const { getByRole } = render(
      <WorldStage
        overlay={worldOverlay()}
        backdrop={null}
        showGrid={false}
        onOpenNote={onOpenNote}
        onOpenCategory={onOpenCategory}
      />,
    );
    const svg = getByRole('img', { name: 'Test world' });

    fireEvent.pointerDown(svg, { pointerId: 1, clientX: 600, clientY: 310 });
    fireEvent.pointerUp(svg, { pointerId: 1, clientX: 600, clientY: 310 });
    expect(onOpenNote).toHaveBeenCalledWith('note-1');

    fireEvent.pointerDown(svg, { pointerId: 2, clientX: 360, clientY: 180 });
    fireEvent.pointerUp(svg, { pointerId: 2, clientX: 360, clientY: 180 });
    expect(onOpenCategory).toHaveBeenCalledWith('region-category');

    fireEvent.pointerDown(svg, { pointerId: 3, clientX: 600, clientY: 310 });
    fireEvent.pointerMove(svg, { pointerId: 3, clientX: 640, clientY: 310 });
    fireEvent.pointerUp(svg, { pointerId: 3, clientX: 640, clientY: 310 });
    expect(onOpenNote).toHaveBeenCalledTimes(1);
  });

  it('opens custom world pins when the SVG drawing is letterboxed', () => {
    worldTranslateY = 50;
    const onOpenNote = vi.fn();
    const { getByRole } = render(
      <WorldStage
        overlay={worldOverlay()}
        backdrop={null}
        showGrid={false}
        onOpenNote={onOpenNote}
        onOpenCategory={vi.fn()}
      />,
    );
    const svg = getByRole('img', { name: 'Test world' });

    fireEvent.pointerDown(svg, { pointerId: 1, clientX: 600, clientY: 360 });
    fireEvent.pointerUp(svg, { pointerId: 1, clientX: 600, clientY: 360 });

    expect(onOpenNote).toHaveBeenCalledWith('note-1');
  });
});

function graphResult(overrides: {
  mode?: GraphAnalysisResult['mode'];
  includeTimeline?: boolean;
  x?: number;
  y?: number;
} = {}): GraphAnalysisResult {
  return {
    mode: overrides.mode ?? 'categories',
    nodes: [{
      id: 'note-1',
      title: 'Note one',
      categories: [],
      x: overrides.x ?? 0.5,
      y: overrides.y ?? 0.5,
      outlier: false,
      no_semantic_signal: false,
      timeline_date: overrides.includeTimeline
        ? { at_ms: 1_700_000_000_000, source_field: 'created', used_fallback: false, date_only: false }
        : undefined,
    }],
    clusters: [],
    semantic_edges: [],
    prior_edges: [],
    provider: { id: 'test', semantic: false },
    summary: {
      note_count: 1,
      embedded_note_count: 0,
      cluster_count: 0,
      outlier_count: 0,
      no_signal_count: 0,
      semantic_edge_candidate_count: 0,
    },
    timeline: overrides.includeTimeline
      ? {
        start_ms: 1_700_000_000_000,
        end_ms: 1_700_000_000_000,
        focus_start_x: 0,
        focus_end_x: 1,
        granularity: 'auto',
        ticks: [],
        undated_count: 0,
        bucket_count: 1,
      }
      : undefined,
  };
}

function svgMatrix(scale: number, translateX: number, translateY: number) {
  return {
    a: scale,
    d: scale,
    e: translateX,
    f: translateY,
    inverse() {
      return svgMatrix(1 / scale, -translateX / scale, -translateY / scale);
    },
  };
}

function worldOverlay(): Overlay {
  return {
    world: {
      id: 'world-1',
      display_name: 'Test world',
      kind: 'image',
      intrinsic_dimensions: [1200, 620],
    },
    pins: [{
      target: { kind: 'note', id: 'note-1', title: 'Note one' },
      point: { kind: 'plane', x: 0.5, y: 0.5 },
    }],
    regions: [{
      category: 'region-category',
      anchor: { kind: 'plane', x: 0.3, y: 0.25 },
      region: { shape: 'bounding_box', x: 0.25, y: 0.2, width: 0.2, height: 0.2 },
    }],
  };
}
