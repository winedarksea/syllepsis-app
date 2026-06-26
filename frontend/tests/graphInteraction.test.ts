import assert from 'node:assert/strict';
import test from 'node:test';
import { findNearestActivatablePoint } from '../src/views/graphInteraction.ts';

test('node hit testing opens the nearest point within the activation radius', () => {
  const points = [
    { id: 'far', x: 40, y: 40 },
    { id: 'near', x: 12, y: 9 },
  ];

  assert.equal(findNearestActivatablePoint(points, { x: 10, y: 10 }, 16), 'near');
});

test('node hit testing ignores pointer releases outside the activation radius', () => {
  const points = [{ id: 'note', x: 40, y: 40 }];

  assert.equal(findNearestActivatablePoint(points, { x: 10, y: 10 }, 16), null);
});
