import assert from 'node:assert/strict';
import test from 'node:test';
import { findNearestActivatablePoint, SvgActivationTracker } from '../src/views/graphInteraction.ts';

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

test('svg activation tracker returns release point for a click gesture', () => {
  const tracker = new SvgActivationTracker();

  tracker.pointerDown({ pointerId: 1, clientX: 10, clientY: 10 });
  const activationPoint = tracker.pointerUp({ pointerId: 1, clientX: 11, clientY: 11 });

  assert.deepEqual(activationPoint, { x: 11, y: 11 });
});

test('svg activation tracker suppresses activation after a drag', () => {
  const tracker = new SvgActivationTracker();

  tracker.pointerDown({ pointerId: 1, clientX: 10, clientY: 10 });
  tracker.pointerMove({ pointerId: 1, clientX: 18, clientY: 10 });
  const activationPoint = tracker.pointerUp({ pointerId: 1, clientX: 18, clientY: 10 });

  assert.equal(activationPoint, null);
});

test('svg activation tracker suppresses activation after multi-pointer gestures', () => {
  const tracker = new SvgActivationTracker();

  tracker.pointerDown({ pointerId: 1, clientX: 10, clientY: 10 });
  tracker.pointerDown({ pointerId: 2, clientX: 15, clientY: 10 });
  tracker.pointerUp({ pointerId: 2, clientX: 15, clientY: 10 });
  const activationPoint = tracker.pointerUp({ pointerId: 1, clientX: 10, clientY: 10 });

  assert.equal(activationPoint, null);
});
