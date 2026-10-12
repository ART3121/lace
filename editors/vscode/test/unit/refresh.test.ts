import * as assert from 'node:assert/strict';

import { Coalescer, ContentMemory } from '../../src/refresh';

describe('Coalescer', () => {
  it('junta os pedidos que chegam durante uma execução numa execução só, depois dela', async () => {
    let runs = 0;
    let release: () => void = () => {};
    const coalescer = new Coalescer(async () => {
      runs++;
      if (runs === 1) {
        await new Promise<void>((resolve) => (release = resolve));
      }
    });
    const first = coalescer.run();
    const second = coalescer.run();
    const third = coalescer.run();
    release();
    await Promise.all([first, second, third]);
    assert.equal(runs, 2);
    await coalescer.run();
    assert.equal(runs, 3);
  });

  it('quem pede durante uma execução espera a seguinte', async () => {
    const order: string[] = [];
    let release: () => void = () => {};
    let runs = 0;
    const coalescer = new Coalescer(async () => {
      runs++;
      order.push(`começa ${runs}`);
      if (runs === 1) {
        await new Promise<void>((resolve) => (release = resolve));
      }
      order.push(`termina ${runs}`);
    });
    void coalescer.run();
    const late = coalescer.run().then(() => order.push('pedido atrasado voltou'));
    release();
    await late;
    assert.deepEqual(order, ['começa 1', 'termina 1', 'começa 2', 'termina 2', 'pedido atrasado voltou']);
  });
});

describe('ContentMemory', () => {
  const bytes = (text: string) => new TextEncoder().encode(text);

  it('uma gravação que não muda o conteúdo não conta', () => {
    const memory = new ContentMemory();
    memory.remember(bytes('{"a":1}'));
    assert.equal(memory.changed(bytes('{"a":1}')), false);
    assert.equal(memory.changed(bytes('{"a":2}')), true);
    assert.equal(memory.changed(bytes('{"a":2}')), false);
  });

  it('sem lembrança, tudo conta', () => {
    const memory = new ContentMemory();
    assert.equal(memory.changed(bytes('x')), true);
    memory.forget();
    assert.equal(memory.changed(bytes('x')), true);
  });
});
