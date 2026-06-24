import assert from 'node:assert/strict';
import test from 'node:test';
import { convexHull, filterSemanticEdges, zoomCameraAtPoint } from '../src/views/graphGeometry.ts';

test('similarity threshold filters edges without changing their order', () => {
  const edges = [
    { source: 'a', target: 'b', similarity: 0.8 },
    { source: 'a', target: 'c', similarity: 0.3 },
    { source: 'b', target: 'c', similarity: 0.5 },
  ];
  assert.deepEqual(filterSemanticEdges(edges, 0.5), [edges[0], edges[2]]);
});

test('zoom keeps the pointed graph coordinate under the cursor', () => {
  const camera = { x: 0, y: 0, zoom: 1 };
  const next = zoomCameraAtPoint(camera, { x: 250, y: 180 }, 2);
  assert.deepEqual(next, { x: 125, y: 90, zoom: 2 });
});

test('convex hull drops interior points', () => {
  const hull = convexHull([
    { x: 0, y: 0 },
    { x: 10, y: 0 },
    { x: 10, y: 10 },
    { x: 0, y: 10 },
    { x: 5, y: 5 },
  ]);
  assert.equal(hull.length, 4);
});
