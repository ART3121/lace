// Que versão do Lace cada recurso da extensão exige. A extensão tem versão
// própria e precisa funcionar com um Lace instalado mais antigo que ela: o
// que a versão instalada não suporta fica desligado.

/**
 * A primeira versão do Lace com cada recurso, no formato de JSON que a
 * extensão lê. A 0.2.0 é a primeira com `lace status`, `lace top`, `-C` e
 * `--events` (a 0.1.0 ainda se chamava Solar); os schemas de `status`,
 * `top`, `add`, `new`, `remove`, `order`, `move`, `build`, `proc` e `error`
 * não mudaram desde ela. O `wave` ganhou campos
 * depois, mas os que a extensão lê (`waveform`, `pid`) estão lá desde a
 * 0.2.0.
 */
export const FEATURES = {
  /** Novo projeto: `lace new <nome> --dir <pasta> --json`. */
  newProject: '0.2.0',
  /** A vista de fontes: `lace status --json`. */
  sources: '0.2.0',
  /** Definir como topo: `lace top <arquivo> --json`. */
  setTop: '0.2.0',
  /** Adicionar ao projeto e Novo arquivo Verilog: `lace add`. */
  add: '0.2.0',
  /** Remover do projeto, a ordem das listas e mover ou renomear: `lace remove`, `order` e `move`. */
  files: '0.2.0',
  /**
   * Registrar o que o explorador do VS Code renomeou: `lace move --after`.
   * Ainda não publicado: a próxima versão depois da 0.6.0. A extensão tenta
   * com qualquer versão e só usa esta para explicar a recusa de uma mais
   * antiga.
   */
  moveAfter: '0.7.0',
  /** Compilar os processadores, criar e configurar: `lace build` e `lace proc`. */
  processors: '0.2.0',
  /** Abrir a onda: `lace wave`. */
  wave: '0.2.0',
  /**
   * Verificar o Verilog: `lace check`. Na 0.2.0 o check ainda compilava os
   * processadores antes e escrevia `builds` no JSON; o formato de hoje
   * (`check` e `report`) é da 0.3.0.
   */
  check: '0.3.0',
  /** Simular: `lace sim`, o projeto, um testbench ou um processador. */
  sim: '0.3.0',
  /** A simulação rápida, no Verilator e sem onda: `lace sim --fast` (e o campo `fast`). */
  simFast: '0.4.0',
  /** Sintetizar: `lace synth`, com o esquemático (`--svg`). */
  synth: '0.3.0',
} as const;

export type Feature = keyof typeof FEATURES;
export type FeatureSet = Record<Feature, boolean>;

/** A versão mais antiga com que a extensão faz alguma coisa. */
export const MIN_VERSION = '0.2.0';

/**
 * A versão na linha do `lace --version` (`lace 0.6.0`, o formato do clap),
 * ou `undefined` se a linha não tem esse formato.
 */
export function parseVersionLine(text: string): string | undefined {
  const match = /^lace (\d+\.\d+\.\d+)(?:[-+][0-9A-Za-z.-]+)?$/.exec(text.trim());
  return match?.[1];
}

/** Compara `X.Y.Z` como números: negativo se `a` é mais antiga que `b`. */
export function compareVersions(a: string, b: string): number {
  const pa = a.split('.').map(Number);
  const pb = b.split('.').map(Number);
  for (let i = 0; i < 3; i++) {
    const diff = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (diff !== 0) {
      return diff;
    }
  }
  return 0;
}

export function supportedFeatures(version: string): FeatureSet {
  const set = {} as FeatureSet;
  for (const [feature, since] of Object.entries(FEATURES) as [Feature, string][]) {
    set[feature] = compareVersions(version, since) >= 0;
  }
  return set;
}
