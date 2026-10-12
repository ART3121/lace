// O que a extensão pede ao Lace. Vistas e comandos dependem só desta
// interface: a implementação de hoje roda a CLI (cliClient.ts), e um
// `lace lsp` futuro entra no lugar dela sem mexer no resto.

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
} from './lace-types';

/**
 * Como terminou um comando do Lace, pelo código de saída (docs/CLI.md,
 * "Códigos de saída").
 */
export type LaceRun<T> =
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
   * `kill` encerra o lace na hora).
   */
  | { status: 'cancelled'; result: T | null }
  /** 2: o Lace não conseguiu rodar. `error.code` é estável; `hint`, o comando que resolve. */
  | { status: 'error'; error: ErrorInfo };

export interface RunOptions {
  /** Cancela o comando: o lace recebe SIGTERM e sai com 130. */
  signal?: AbortSignal;
}

export interface CheckOptions extends RunOptions {
  /** Só os módulos deste arquivo como raiz (`lace check <arquivo>`). */
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
  /** Simula este testbench, que passa a ser o do projeto (`lace sim <testbench>`). */
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

/** Onde pôr um arquivo na lista dele (`lace order`). */
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

export interface LaceClient {
  /** A versão do lace, como `0.6.0`. */
  version(): Promise<string>;
  /** `lace new`: cria `<dir>/<nome>/<nome>.spf`. Não precisa de projeto. */
  newProject(name: string, dir: string, options?: RunOptions): Promise<LaceRun<NewReport>>;
  /** `lace status`: o projeto inteiro. `spf` é o `.spf` do projeto. */
  status(spf: string, options?: RunOptions): Promise<LaceRun<StatusReport>>;
  /** `lace top <arquivo>`: escolhe o arquivo de topo. */
  setTop(spf: string, file: string, options?: RunOptions): Promise<LaceRun<TopReport>>;
  /** `lace add`: registra os arquivos; os que não existem saem do modelo. */
  add(spf: string, files: readonly string[], options?: AddOptions): Promise<LaceRun<AddReport>>;
  /** `lace check`: o Icarus sobre o design e cada testbench, sem compilar processadores. */
  check(spf: string, options?: CheckOptions): Promise<LaceRun<CheckReport>>;
  /**
   * `lace wave`: abre a onda no surfer-aurora e volta, com o Surfer aberto.
   * Sem `spf`, roda fora de projeto (só para um arquivo).
   */
  wave(spf: string | undefined, target: WaveTarget, options?: RunOptions): Promise<LaceRun<WaveReport>>;
  /** `lace remove`: tira os arquivos do projeto (continuam no disco). */
  remove(spf: string, files: readonly string[], options?: RunOptions): Promise<LaceRun<RemoveReport>>;
  /** `lace order`: muda a posição do arquivo na lista dele, a ordem de compilação. */
  order(spf: string, file: string, position: ListPosition, options?: RunOptions): Promise<LaceRun<OrderReport>>;
  /**
   * `lace move`: move ou renomeia, e os registrados continuam no projeto.
   * Com `after`, o arquivo já foi movido (pelo explorador do VS Code) e o
   * Lace só atualiza o `.spf` (`lace move --after`).
   */
  move(spf: string, from: string, to: string, options?: RunOptions & { after?: boolean }): Promise<LaceRun<MoveReport>>;
  /** `lace build`: compila os processadores com o YANC. */
  build(spf: string, options?: BuildOptions): Promise<LaceRun<BuildReport>>;
  /** `lace proc add`: cria o processador, com o fonte-modelo. */
  addProcessor(spf: string, name: string, language: 'cmm' | 'cpp', options?: RunOptions): Promise<LaceRun<Processor>>;
  /** `lace proc set`: frequência, clocks e exportação de arrays, para o próximo build. */
  configureProcessor(spf: string, name: string, changes: ProcessorChanges, options?: RunOptions): Promise<LaceRun<Processor>>;
  /** `lace sim`: compila os processadores e simula. */
  sim(spf: string, options?: SimOptions): Promise<LaceRun<SimReport>>;
  /** `lace synth`: compila os processadores e sintetiza o topo no Yosys. */
  synth(spf: string, options?: SynthOptions): Promise<LaceRun<SynthReport>>;
}

/**
 * O lace não pôde ser executado, ou saiu de um jeito que o contrato da CLI
 * não prevê: código de saída desconhecido, morte por sinal, saída que não é
 * JSON. Um erro do Lace (código 2) não é isto: volta como `status: 'error'`.
 */
export class LaceProcessError extends Error {
  constructor(
    message: string,
    /** O código de saída, ou `null` se o processo nem começou ou morreu por sinal. */
    readonly exitCode: number | null = null,
    /** O que o lace escreveu no stderr. */
    readonly stderr = '',
  ) {
    super(message);
    this.name = 'LaceProcessError';
  }
}
