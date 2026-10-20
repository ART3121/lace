// O canal JSON-RPC por linhas, sem processo: os dois lados são PassThrough.

import * as assert from 'node:assert/strict';
import { PassThrough } from 'node:stream';

import { RpcChannel, RpcClosedError, type RpcNotification } from '../../src/lace/jsonRpc';

interface Harness {
  channel: RpcChannel;
  /** O que o cliente escreveu, uma mensagem por linha, já em objetos. */
  sent: () => Promise<unknown[]>;
  /** O servidor escreve uma linha. */
  reply: (line: string) => void;
  /** O servidor fecha a saída. */
  end: () => void;
  notifications: RpcNotification[];
  warnings: string[];
}

function harness(): Harness {
  const toServer = new PassThrough();
  const fromServer = new PassThrough();
  const notifications: RpcNotification[] = [];
  const warnings: string[] = [];
  let written = '';
  toServer.setEncoding('utf8');
  toServer.on('data', (chunk: string) => {
    written += chunk;
  });
  const channel = new RpcChannel(toServer, fromServer, {
    onNotification: (n) => notifications.push(n),
    onProtocolWarning: (w) => warnings.push(w),
  });
  return {
    channel,
    sent: async () => {
      await tick();
      return written
        .split('\n')
        .filter((l) => l !== '')
        .map((l) => JSON.parse(l));
    },
    reply: (line) => fromServer.write(line),
    end: () => fromServer.end(),
    notifications,
    warnings,
  };
}

function tick(): Promise<void> {
  return new Promise((resolve) => setImmediate(resolve));
}

describe('RpcChannel', () => {
  it('numera os pedidos a partir de 1 e escreve uma linha por mensagem', async () => {
    const h = harness();
    const a = h.channel.request('initialize', { client: 'x' });
    const b = h.channel.request('toolchain.info');
    h.channel.notify('$/cancel', { id: a.id });
    assert.equal(a.id, 1);
    assert.equal(b.id, 2);
    assert.deepEqual(await h.sent(), [
      { jsonrpc: '2.0', id: 1, method: 'initialize', params: { client: 'x' } },
      { jsonrpc: '2.0', id: 2, method: 'toolchain.info' },
      { jsonrpc: '2.0', method: '$/cancel', params: { id: 1 } },
    ]);
    assert.ok(h.channel.busy);
  });

  it('entrega a resposta ao pedido com o mesmo id, mesmo fora de ordem e partida em pedaços', async () => {
    const h = harness();
    const a = h.channel.request('a');
    const b = h.channel.request('b');
    h.reply('{"jsonrpc":"2.0","id":2,"res');
    h.reply('ult":{"b":true}}\r\n{"jsonrpc":"2.0","id":1,"result":{"a":true}}\n');
    assert.deepEqual(await b.reply, { result: { b: true } });
    assert.deepEqual(await a.reply, { result: { a: true } });
    assert.ok(!h.channel.busy);
  });

  it('uma notificação entre o pedido e a resposta chega antes dela', async () => {
    const h = harness();
    const order: string[] = [];
    const a = h.channel.request('flow.run');
    void a.reply.then(() => order.push('reply'));
    h.reply('{"jsonrpc":"2.0","method":"events","params":{"id":1,"events":[]}}\n');
    h.reply('{"jsonrpc":"2.0","id":1,"result":{}}\n');
    await a.reply;
    await tick();
    assert.deepEqual(h.notifications, [{ method: 'events', params: { id: 1, events: [] } }]);
    assert.deepEqual(order, ['reply']);
  });

  it('um erro vira { error } com código, mensagem e data', async () => {
    const h = harness();
    const a = h.channel.request('project.status', {});
    h.reply('{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"x","data":{"code":"project_not_found"}}}\n');
    assert.deepEqual(await a.reply, {
      error: { code: -32000, message: 'x', data: { code: 'project_not_found' } },
    });
  });

  it('linha que não é JSON, resposta sem pedido e erro sem id viram avisos, não quedas', async () => {
    const h = harness();
    h.reply('isto nao e json\n');
    h.reply('{"jsonrpc":"2.0","id":99,"result":1}\n');
    h.reply('{"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}}\n');
    h.reply('42\n');
    await tick();
    assert.equal(h.warnings.length, 4, h.warnings.join('; '));
    assert.match(h.warnings[0], /não é JSON/);
    assert.match(h.warnings[1], /desconhecido/);
    assert.match(h.warnings[2], /Invalid JSON/);
    assert.ok(!h.channel.isClosed);
  });

  it('o fim da saída rejeita os pedidos pendentes e recusa os seguintes', async () => {
    const h = harness();
    const a = h.channel.request('a');
    h.reply('{"jsonrpc":"2.0","id":1,"resu');
    h.end();
    await assert.rejects(a.reply, (error: unknown) => {
      assert.ok(error instanceof RpcClosedError);
      assert.match(error.reason, /fechou/);
      return true;
    });
    assert.ok(h.channel.isClosed);
    // A linha pela metade é um aviso.
    assert.ok(h.warnings.some((w) => /não é JSON/.test(w)));
    await assert.rejects(h.channel.request('b').reply, RpcClosedError);
  });
});
