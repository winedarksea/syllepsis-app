import { describe, expect, it } from 'vitest';
import { detectAccidentalWholeNoteCodeFence } from './wholeNoteFence';

describe('detectAccidentalWholeNoteCodeFence', () => {
  it('detects an unlabeled four-backtick wrapper around prose markdown', () => {
    const body = [
      '````',
      '# TODO',
      '',
      'Regular prose.',
      '',
      '```python',
      'import pandas as pd',
      '```',
      '````',
    ].join('\n');

    const detected = detectAccidentalWholeNoteCodeFence(body);

    expect(detected).not.toBeNull();
    expect(detected?.fenceLength).toBe(4);
    expect(detected?.language).toBe('');
    expect(detected?.innerMarkdown).toContain('# TODO');
  });

  it('does not flag a real language code note', () => {
    const body = [
      '```python',
      'def main():',
      '    return 1',
      '```',
    ].join('\n');

    expect(detectAccidentalWholeNoteCodeFence(body)).toBeNull();
  });

  it('preserves inner triple-backtick code blocks when unwrapped', () => {
    const body = [
      '````markdown',
      '# Notes',
      '',
      '```python',
      'print("nested")',
      '```',
      '````',
    ].join('\n');

    const detected = detectAccidentalWholeNoteCodeFence(body);

    expect(detected?.innerMarkdown).toBe([
      '# Notes',
      '',
      '```python',
      'print("nested")',
      '```',
    ].join('\n'));
  });

  it('does not flag plain unlabeled code without prose markdown signals', () => {
    const body = [
      '```',
      'const value = 1;',
      'console.log(value);',
      '```',
    ].join('\n');

    expect(detectAccidentalWholeNoteCodeFence(body)).toBeNull();
  });
});
