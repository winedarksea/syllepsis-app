import assert from 'node:assert/strict';
import { readFileSync, statSync } from 'node:fs';
import test from 'node:test';

const LOW_DETAIL_PATH = new URL('../src/assets/earth/countries-equal-earth-low.svg', import.meta.url);
const HIGH_DETAIL_PATH = new URL('../src/assets/earth/countries-equal-earth-high.svg', import.meta.url);

test('bundled Earth SVGs remain within the recorded size budget', () => {
  assert.ok(statSync(LOW_DETAIL_PATH).size <= 200_000, 'global layer exceeded 200 KB');
  assert.ok(statSync(HIGH_DETAIL_PATH).size <= 8_000_000, '1:10m layer exceeded 8 MB');
});

test('bundled Earth layers contain country geometry and provenance', () => {
  for (const path of [LOW_DETAIL_PATH, HIGH_DETAIL_PATH]) {
    const svg = readFileSync(path, 'utf8');
    assert.match(svg, /data-source="Natural Earth"/);
    assert.ok((svg.match(/<path /g) ?? []).length >= 170);
  }
});
