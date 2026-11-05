// O que evita reler o projeto em ciclo, sem depender do VS Code. Reler roda
// `aurora status`, que lê o .spf; se uma releitura mudasse o .spf, o vigia
// avisaria de novo, e assim por diante. Como no watcher.rs do Studio, só
// mudança de verdade conta: aqui, conteúdo diferente do da última leitura.

import { createHash } from 'node:crypto';

/**
 * Junta os pedidos de uma tarefa: se ela já está rodando, o pedido novo vira
 * uma execução só, depois da atual. Quem pede espera até uma execução que
 * começou depois do pedido terminar.
 */
export class Coalescer {
  private current: Promise<void> | undefined;
  private again = false;

  constructor(private readonly task: () => Promise<void>) {}

  run(): Promise<void> {
    if (this.current) {
      this.again = true;
      return this.current;
    }
    this.current = this.loop();
    return this.current;
  }

  private async loop(): Promise<void> {
    try {
      do {
        this.again = false;
        await this.task();
      } while (this.again);
    } finally {
      this.current = undefined;
    }
  }
}

/** Lembra o conteúdo da última leitura do .spf, pelo SHA-256. */
export class ContentMemory {
  private last: string | undefined;

  /** Guarda o conteúdo lido agora. */
  remember(content: Uint8Array): void {
    this.last = digest(content);
  }

  /** O conteúdo é diferente do último lembrado; se for, passa a ser ele. */
  changed(content: Uint8Array): boolean {
    const now = digest(content);
    if (now === this.last) {
      return false;
    }
    this.last = now;
    return true;
  }

  forget(): void {
    this.last = undefined;
  }
}

function digest(content: Uint8Array): string {
  return createHash('sha256').update(content).digest('hex');
}
