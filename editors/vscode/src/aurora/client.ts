// O que a extensão pede à AURORA. Vistas e comandos dependem só desta
// interface: uma implementação roda a CLI, um processo por comando
// (cliClient.ts); a outra fala com um `aurora serve` em segundo plano
// (serveClient.ts). A conexão escolhe (connection.ts, `aurora.transport`).

import type {
  AddReport,
  BuildReport,
  CheckReport,
  ErrorInfo,
  Event,
  MoveReport,
  NewReport,
  OrderReport,
  Processor,
  RemoveReport,
  SimReport,
  StatusReport,
  SynthReport,
  TopReport,
  WaveReport,
} from './aurora-types';

/**
 * Como terminou um comando da AURORA, pelo código de saída (docs/CLI.md,
 * "Códigos de saída").
 */
export type AuroraRun<T> =
  /** 0: deu certo. */
  | { status: 'succeeded'; result: T }
  /**
   * 1: a operação rodou e falhou (erro de compilação, simulação que falhou).
   * O resultado diz por quê.
   */
  | { status: 'failed'; result: T }
  /**
   * 130: cancelado. O resultado traz `status: cancelled` no que estava
   * rodando; é `null` quando o processo saiu sem escrevê-lo (no Windows, o
   * `kill` encerra o aurora na hora).
   */
  | { status: 'cancelled'; result: T | null }
  /** 2: a AURORA não conseguiu rodar. `error.code` é estável; `hint`, o comando que resolve. */
  | { status: 'error'; error: ErrorInfo };

export interface RunOptions {
  /** Cancela o comando: o aurora recebe SIGTERM e sai com 130. */
  signal?: AbortSignal;
}

export interface CheckOptions extends RunOptions {
  /** Só os módulos deste arquivo como raiz (`aurora check <arquivo>`). */
  file?: string;
  /** Só o Verilog deste processador e o testbench dele (`-p`). */
  processor?: string;
  /** Também o `verilator --lint-only` (`--lint`). */
  lint?: boolean;
  /** Cada evento das ferramentas, enquanto rodam. */
  onEvent?: (event: Event) => void;
}

/** Que onda abrir. */
export type WaveTarget =
  /** A do testbench simulado do projeto. */
  | { kind: 'project' }
  /** A da simulação de um processador (`-p`). */
  | { kind: 'processor'; name: string }
  /** Um arquivo VCD, FST ou GHW. */
  | { kind: 'file'; path: string };

/** As opções das operações que rodam ferramentas e avisam cada evento. */
export interface EventOptions extends RunOptions {
  onEvent?: (event: Event) => void;
}

export interface SimOptions extends EventOptions {
  /** Simula este testbench, que passa a ser o do projeto (`aurora sim <testbench>`). */
  testbench?: string;
  /** Compila e simula o processador com o testbench do YANC (`-p`). */
  processor?: string;
  /** No Verilator, sem gravar onda (`--fast`). */
  fast?: boolean;
  /** No Verilator, com onda (`--verilator`). */
  verilator?: boolean;
  /** Encerra a simulação que passar disto, em segundos (`--timeout`). */
  timeoutSeconds?: number;
}

export interface SynthOptions extends EventOptions {
  /** Compila e sintetiza só o processador (`-p`). */
  processor?: string;
  /** Desenha o esquemático em SVG (`--svg`). */
  svg?: boolean;
}

export interface BuildOptions extends EventOptions {
  /** Só estes processadores (`-p` para cada um); sem eles, todos. */
  processors?: readonly string[];
}

/** Onde pôr um arquivo na lista dele (`aurora order`). */
export type ListPosition =
  | { kind: 'first' }
  | { kind: 'last' }
  | { kind: 'before'; other: string }
  | { kind: 'after'; other: string };

export interface ProcessorChanges {
  /** Frequência de operação, em MHz (`--freq`). */
  frequencyMhz?: number;
  /** Clocks a simular (`--clocks`). */
  clocks?: number;
  /** Exporta os arrays para a simulação (`--arrays`). */
  showArrays?: boolean;
}

export interface AddOptions extends RunOptions {
  /** Registra como testbench (`--tb`), em vez de decidir pelo conteúdo. */
  testbench?: boolean;
}

export interface AuroraClient {
  /** A versão do aurora, como `0.6.0`. */
  version(): Promise<string>;
  /** `aurora new`: cria `<dir>/<nome>/<nome>.spf`. Não precisa de projeto. */
  newProject(name: string, dir: string, options?: RunOptions): Promise<AuroraRun<NewReport>>;
  /** `aurora status`: o projeto inteiro. `spf` é o `.spf` do projeto. */
  status(spf: string, options?: RunOptions): Promise<AuroraRun<StatusReport>>;
  /** `aurora top <arquivo>`: escolhe o arquivo de topo. */
  setTop(spf: string, file: string, options?: RunOptions): Promise<AuroraRun<TopReport>>;
  /** `aurora add`: registra os arquivos; os que não existem saem do modelo. */
  add(spf: string, files: readonly string[], options?: AddOptions): Promise<AuroraRun<AddReport>>;
  /** `aurora check`: o Icarus sobre o design e cada testbench, sem compilar processadores. */
  check(spf: string, options?: CheckOptions): Promise<AuroraRun<CheckReport>>;
  /**
   * `aurora wave`: abre a onda no surfer-aurora e volta, com o Surfer aberto.
   * Sem `spf`, roda fora de projeto (só para um arquivo).
   */
  wave(spf: string | undefined, target: WaveTarget, options?: RunOptions): Promise<AuroraRun<WaveReport>>;
  /** `aurora remove`: tira os arquivos do projeto (continuam no disco). */
  remove(spf: string, files: readonly string[], options?: RunOptions): Promise<AuroraRun<RemoveReport>>;
  /** `aurora order`: muda a posição do arquivo na lista dele, a ordem de compilação. */
  order(spf: string, file: string, position: ListPosition, options?: RunOptions): Promise<AuroraRun<OrderReport>>;
  /**
   * `aurora move`: move ou renomeia, e os registrados continuam no projeto.
   * Com `after`, o arquivo já foi movido (pelo explorador do VS Code) e o
   * AURORA só atualiza o `.spf` (`aurora move --after`).
   */
  move(spf: string, from: string, to: string, options?: RunOptions & { after?: boolean }): Promise<AuroraRun<MoveReport>>;
  /** `aurora build`: compila os processadores com o YANC. */
  build(spf: string, options?: BuildOptions): Promise<AuroraRun<BuildReport>>;
  /** `aurora proc add`: cria o processador, com o fonte-modelo. */
  addProcessor(spf: string, name: string, language: 'cmm' | 'cpp', options?: RunOptions): Promise<AuroraRun<Processor>>;
  /** `aurora proc set`: frequência, clocks e exportação de arrays, para o próximo build. */
  configureProcessor(spf: string, name: string, changes: ProcessorChanges, options?: RunOptions): Promise<AuroraRun<Processor>>;
  /** `aurora sim`: compila os processadores e simula. */
  sim(spf: string, options?: SimOptions): Promise<AuroraRun<SimReport>>;
  /** `aurora synth`: compila os processadores e sintetiza o topo no Yosys. */
  synth(spf: string, options?: SynthOptions): Promise<AuroraRun<SynthReport>>;
  /** Solta o que o cliente mantém (o `aurora serve` em segundo plano). */
  dispose?(): Promise<void> | void;
}

/**
 * O aurora não pôde ser executado, ou saiu de um jeito que o contrato da CLI
 * não prevê: código de saída desconhecido, morte por sinal, saída que não é
 * JSON. Um erro da AURORA (código 2) não é isto: volta como `status: 'error'`.
 */
export class AuroraProcessError extends Error {
  constructor(
    message: string,
    /** O código de saída, ou `null` se o processo nem começou ou morreu por sinal. */
    readonly exitCode: number | null = null,
    /** O que o aurora escreveu no stderr. */
    readonly stderr = '',
  ) {
    super(message);
    this.name = 'AuroraProcessError';
  }
}
