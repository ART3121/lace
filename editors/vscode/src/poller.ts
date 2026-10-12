// Confere de tempos em tempos a data de alguns arquivos, para o que o vigia
// do VS Code (createFileSystemWatcher) não vê. Quando uma árvore de pastas é
// criada e já recebe um arquivo no mesmo instante, o vigia não passa a olhar
// as subpastas dela, nem depois: é o que o `lace synth` faz em
// `.lace/Temp/synth/<alvo>/` na primeira síntese de um alvo. Um `stat` por
// arquivo a cada dois segundos não falha assim.

import { stat } from 'node:fs/promises';

/** O que mudou entre duas passadas, sem depender do VS Code. */
export class ModifiedTracker {
  private readonly known = new Map<string, number>();
  private primed = false;

  /**
   * Os arquivos que mudaram (data nova) ou apareceram desde a última
   * passada. A primeira passada só aprende as datas: o que já existia ao
   * começar não conta como mudança.
   */
  update(current: ReadonlyMap<string, number>): string[] {
    const changed: string[] = [];
    for (const [path, modified] of current) {
      const before = this.known.get(path);
      if (this.primed && before !== modified) {
        changed.push(path);
      }
      this.known.set(path, modified);
    }
    for (const path of [...this.known.keys()]) {
      if (!current.has(path)) {
        this.known.delete(path);
      }
    }
    this.primed = true;
    return changed;
  }
}

export class ChangePoller {
  private tracker = new ModifiedTracker();
  private timer: NodeJS.Timeout | undefined;
  private busy = false;

  constructor(
    /** Os arquivos a conferir nesta passada (os que não existem são ignorados). */
    private readonly paths: () => Promise<string[]>,
    private readonly onChange: (path: string) => void,
    private readonly intervalMs = 2000,
  ) {}

  start(): void {
    if (!this.timer) {
      void this.poll();
      this.timer = setInterval(() => void this.poll(), this.intervalMs);
    }
  }

  /** Esquece as datas: a próxima passada só aprende (outro projeto ficou ativo). */
  reset(): void {
    this.tracker = new ModifiedTracker();
  }

  dispose(): void {
    clearInterval(this.timer);
    this.timer = undefined;
  }

  private async poll(): Promise<void> {
    if (this.busy) {
      return;
    }
    this.busy = true;
    try {
      const current = new Map<string, number>();
      for (const path of await this.paths()) {
        try {
          current.set(path, (await stat(path)).mtimeMs);
        } catch {
          // ainda não existe
        }
      }
      this.tracker.update(current).forEach((path) => this.onChange(path));
    } finally {
      this.busy = false;
    }
  }
}
