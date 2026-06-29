import assert from 'node:assert/strict';
import test from 'node:test';
import {
  formatTimelineDateSource,
  formatTimelineNodeDate,
} from '../src/views/timelinePresentation.ts';

test('date-only timeline values do not shift across local time zones', () => {
  const formatted = formatTimelineNodeDate({
    at_ms: Date.UTC(2024, 4, 10),
    source_field: 'completed',
    used_fallback: false,
    date_only: true,
  }, 'en-US');
  assert.equal(formatted, 'May 10, 2024');
});

test('timestamp timeline values include local time', () => {
  const formatted = formatTimelineNodeDate({
    at_ms: Date.UTC(2024, 4, 10, 14, 30),
    source_field: 'created',
    used_fallback: false,
    date_only: false,
  }, 'en-US');
  assert.match(formatted, /2024/);
  assert.match(formatted, /:/);
});

test('fallback source is identified', () => {
  assert.equal(formatTimelineDateSource({
    at_ms: 0,
    source_field: 'created',
    used_fallback: true,
    date_only: false,
  }), 'created fallback');
});
