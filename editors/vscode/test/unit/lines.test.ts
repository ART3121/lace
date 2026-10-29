import * as assert from 'node:assert/strict';

import { LineSplitter } from '../../src/aurora/lines';

describe('LineSplitter', () => {
  it('só entrega a linha quando o \\n dela chega', () => {
    const lines = new LineSplitter();
    assert.deepEqual(lines.push('{"event":"st'), []);
    assert.deepEqual(lines.push('ep_started"}\n{"a"'), ['{"event":"step_started"}']);
    assert.deepEqual(lines.push(':1}\n'), ['{"a":1}']);
    assert.deepEqual(lines.end(), []);
  });

  it('tira o \\r do \\r\\n', () => {
    const lines = new LineSplitter();
    assert.deepEqual(lines.push('um\r\ndois\r\n'), ['um', 'dois']);
  });

  it('entrega no fim a última linha sem \\n', () => {
    const lines = new LineSplitter();
    assert.deepEqual(lines.push('um\ndois'), ['um']);
    assert.deepEqual(lines.end(), ['dois']);
    assert.deepEqual(lines.end(), []);
  });

  it('várias linhas num pedaço só, e linhas vazias', () => {
    const lines = new LineSplitter();
    assert.deepEqual(lines.push('a\n\nb\n'), ['a', '', 'b']);
  });
});
