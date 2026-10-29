// O enquadramento do `aurora serve`: JSON-RPC 2.0 com uma mensagem por linha
// sobre um par de streams. Sem processo aqui: quem abre o `aurora serve` passa
// o stdin e o stdout dele, e os testes passam `PassThrough`.

import type { Readable, Writable } from 'node:stream';

import { LineSplitter } from './lines';

/** Um erro do JSON-RPC: o `error` de uma resposta. */
export interface RpcError {
  code: number;
  message: string;
  data?: unknown;
}

/** Uma notificação do servidor: sem `id`, com `method` e `params`. */
export interface RpcNotification {
  method: string;
  params: unknown;
}

/** O que uma resposta traz: o resultado ou o erro. */
export type RpcReply = { result: unknown } | { error: RpcError };

/**
 * O canal fechou (o processo saiu, ou o stream acabou) com pedidos sem
 * resposta. `reason` é o que quem fechou disse.
 */
export class RpcClosedError extends Error {
  constructor(readonly reason: string) {
    super(reason);
    this.name = 'RpcClosedError';
  }
}

export interface RpcChannelOptions {
  /** Cada notificação, na ordem em que chega e antes da resposta que a segue. */
  onNotification?: (notification: RpcNotification) => void;
  /** Uma linha do stdout que não é JSON, ou uma resposta sem pedido. */
  onProtocolWarning?: (message: string) => void;
}

interface Pending {
  resolve: (reply: RpcReply) => void;
  reject: (error: Error) => void;
}

/**
 * Um lado cliente do JSON-RPC por linhas. Os ids são números crescentes;
 * uma resposta resolve a promessa do pedido com o mesmo id; uma linha sem
 * `id` é notificação. Fechar rejeita os pedidos pendentes.
 */
export class RpcChannel {
  private nextId = 1;
  private readonly pending = new Map<number, Pending>();
  private readonly lines = new LineSplitter();
  private closed: RpcClosedError | undefined;

  constructor(
    private readonly input: Writable,
    output: Readable,
    private readonly options: RpcChannelOptions = {},
  ) {
    output.setEncoding('utf8');
    output.on('data', (chunk: string) => {
      this.lines.push(chunk).forEach((line) => this.take(line));
    });
    output.on('end', () => {
      this.lines.end().forEach((line) => this.take(line));
      this.close('a saída do aurora serve fechou');
    });
    output.on('error', (error: Error) => this.close(`a saída do aurora serve falhou: ${error.message}`));
    input.on('error', (error: Error) => this.close(`a entrada do aurora serve falhou: ${error.message}`));
  }

  /** Há pedidos sem resposta. */
  get busy(): boolean {
    return this.pending.size > 0;
  }

  /** O canal já fechou. */
  get isClosed(): boolean {
    return this.closed !== undefined;
  }

  /**
   * Manda um pedido e devolve o `id` e a promessa da resposta. A promessa
   * rejeita com `RpcClosedError` se o canal fechar antes. `before` roda com
   * o `id` antes de a mensagem sair: as notificações do pedido podem chegar
   * durante a escrita, e quem as trata precisa já conhecer o id.
   */
  request(method: string, params?: unknown, before?: (id: number) => void): { id: number; reply: Promise<RpcReply> } {
    const id = this.nextId++;
    if (this.closed) {
      return { id, reply: Promise.reject(this.closed) };
    }
    const reply = new Promise<RpcReply>((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
    });
    before?.(id);
    const message: Record<string, unknown> = { jsonrpc: '2.0', id, method };
    if (params !== undefined) {
      message.params = params;
    }
    this.write(message);
    return { id, reply };
  }

  /** Manda uma notificação (sem resposta). */
  notify(method: string, params?: unknown): void {
    if (this.closed) {
      return;
    }
    const message: Record<string, unknown> = { jsonrpc: '2.0', method };
    if (params !== undefined) {
      message.params = params;
    }
    this.write(message);
  }

  /** Fecha o canal: os pedidos pendentes rejeitam com `reason`. */
  close(reason: string): void {
    if (this.closed) {
      return;
    }
    this.closed = new RpcClosedError(reason);
    for (const pending of this.pending.values()) {
      pending.reject(this.closed);
    }
    this.pending.clear();
  }

  private write(message: Record<string, unknown>): void {
    try {
      this.input.write(`${JSON.stringify(message)}\n`, (error) => {
        if (error) {
          this.close(`não deu para escrever no aurora serve: ${error.message}`);
        }
      });
    } catch (error) {
      this.close(`não deu para escrever no aurora serve: ${(error as Error).message}`);
    }
  }

  private take(line: string): void {
    if (line.trim() === '') {
      return;
    }
    let value: unknown;
    try {
      value = JSON.parse(line);
    } catch {
      this.options.onProtocolWarning?.(`uma linha do aurora serve não é JSON: ${clip(line)}`);
      return;
    }
    if (typeof value !== 'object' || value === null) {
      this.options.onProtocolWarning?.(`uma linha do aurora serve não é um objeto: ${clip(line)}`);
      return;
    }
    const message = value as { id?: unknown; method?: unknown; params?: unknown; result?: unknown; error?: unknown };
    if (message.id === undefined || message.id === null) {
      if (typeof message.method === 'string') {
        try {
          this.options.onNotification?.({ method: message.method, params: message.params });
        } catch (error) {
          this.options.onProtocolWarning?.(`erro ao tratar ${message.method}: ${String(error)}`);
        }
      } else if (message.error !== undefined) {
        // Um erro sem id: JSON inválido ou pedido malformado. Não há a quem
        // entregar; fica no log.
        const error = message.error as Partial<RpcError>;
        this.options.onProtocolWarning?.(`o aurora serve recusou uma mensagem: ${error.message ?? clip(line)}`);
      }
      return;
    }
    const pending = typeof message.id === 'number' ? this.pending.get(message.id) : undefined;
    if (!pending) {
      this.options.onProtocolWarning?.(`resposta do aurora serve para um pedido desconhecido: ${clip(line)}`);
      return;
    }
    this.pending.delete(message.id as number);
    if (message.error !== undefined) {
      const error = message.error as Partial<RpcError>;
      pending.resolve({
        error: {
          code: typeof error.code === 'number' ? error.code : 0,
          message: typeof error.message === 'string' ? error.message : clip(line),
          data: error.data,
        },
      });
    } else {
      pending.resolve({ result: message.result });
    }
  }
}

export function clip(text: string): string {
  return text.length > 200 ? `${text.slice(0, 200)}...` : text;
}
