import * as assert from 'node:assert/strict';

import { ModifiedTracker } from '../../src/poller';

describe('ModifiedTracker', () => {
  it('a primeira passada só aprende: o que já existia não é mudança', () => {
    const tracker = new ModifiedTracker();
    assert.deepEqual(tracker.update(new Map([['a.html', 1]])), []);
    assert.deepEqual(tracker.update(new Map([['a.html', 1]])), []);
  });

  it('data nova e arquivo novo contam; arquivo que sumiu e volta também', () => {
    const tracker = new ModifiedTracker();
    tracker.update(new Map([['a.html', 1]]));
    assert.deepEqual(tracker.update(new Map([['a.html', 2]])), ['a.html']);
    assert.deepEqual(tracker.update(new Map([['a.html', 2], ['b.html', 5]])), ['b.html']);
    assert.deepEqual(tracker.update(new Map([['b.html', 5]])), []);
    assert.deepEqual(tracker.update(new Map([['a.html', 2], ['b.html', 5]])), ['a.html']);
  });
});
