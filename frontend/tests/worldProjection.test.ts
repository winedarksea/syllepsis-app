import assert from 'node:assert/strict';
import test from 'node:test';
import {
  equalEarthForward,
  equalEarthInverseNormalized,
  equalEarthNormalized,
} from '../src/views/worldProjection.ts';

test('Equal Earth keeps the origin centered', () => {
  assert.deepEqual(equalEarthForward(0, 0), [0, 0]);
  assert.deepEqual(equalEarthNormalized(0, 0), [0.5, 0.5]);
});

test('Equal Earth normalized forward/inverse round trips representative points', () => {
  for (const [longitude, latitude] of [
    [0, 0],
    [-122.3321, 47.6062],
    [2.3522, 48.8566],
    [179.5, -45],
    [-180, 0],
  ]) {
    const normalized = equalEarthNormalized(longitude, latitude);
    const inverse = equalEarthInverseNormalized(...normalized);
    assert.ok(inverse);
    assert.ok(Math.abs(inverse[0] - longitude) < 1e-8);
    assert.ok(Math.abs(inverse[1] - latitude) < 1e-8);
  }
});
