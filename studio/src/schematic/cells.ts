// As células do Yosys no PRISM: a família de cada tipo (que dá a cor), o
// símbolo (que dá o desenho) e a geometria, com as portas em posição fixa
// para o ELK ligar os fios no ponto certo de cada desenho.
//
// As células são as genéricas de depois de `proc` e `opt_clean` ($add, $mux,
// $dff, $memrd...); as de porta lógica ($_AND_...) só aparecem num netlist
// mapeado, e caem nos mesmos símbolos.

/** As famílias de célula, que dão a cor e a legenda. */
export type Family = 'arith' | 'logic' | 'compare' | 'mux' | 'reg' | 'mem' | 'module' | 'other';

/** A família de um nó; portas, constantes e fatias têm a sua, fora da legenda. */
export type Category = Family | 'port' | 'const' | 'wiring';

/** As famílias que a legenda mostra, na ordem dela. */
export const LEGEND: Family[] = ['arith', 'logic', 'compare', 'mux', 'reg', 'mem', 'module', 'other'];

export type Gate = 'and' | 'or' | 'xor' | 'nand' | 'nor' | 'xnor' | 'not' | 'buf';

export type Shape =
  | { kind: 'in' }
  | { kind: 'out' }
  | { kind: 'inout' }
  | { kind: 'gate'; gate: Gate; glyph: string | null }
  | { kind: 'op'; glyph: string }
  | { kind: 'mux'; parallel: boolean }
  | { kind: 'reg'; title: string; clockInverted: boolean }
  | { kind: 'box' }
  | { kind: 'split' }
  | { kind: 'join' };

export type Side = 'W' | 'E' | 'S' | 'N';

/** Uma porta de um símbolo, relativa ao canto do nó. */
export interface PortGeometry {
  name: string;
  side: Side;
  x: number;
  y: number;
  /** O texto ao lado da porta, dentro do símbolo. */
  label: string | null;
  output: boolean;
}

export interface Geometry {
  width: number;
  height: number;
  ports: PortGeometry[];
}

// Medidas ---------------------------------------------------------------------

/** O texto do PRISM é monoespaçado: a largura sai da conta, sem medir. A
 * JetBrains Mono avança 0,6 em por caractere. */
export const FONT = { label: 10, title: 11, glyph: 14 } as const;
export const textWidth = (text: string, size: number = FONT.label) => Math.ceil(text.length * size * 0.6);

const PITCH = 16;
const SLICE_PITCH = 14;
const round4 = (value: number) => Math.ceil(value / 4) * 4;

// Tipos -----------------------------------------------------------------------

const GATES: Record<string, [Gate, string | null]> = {
  $and: ['and', null],
  $or: ['or', null],
  $xor: ['xor', null],
  $xnor: ['xnor', null],
  $not: ['not', null],
  $reduce_and: ['and', '&'],
  $reduce_or: ['or', '|'],
  $reduce_xor: ['xor', '^'],
  $reduce_xnor: ['xnor', '~^'],
  $reduce_bool: ['or', '!='],
  $logic_and: ['and', '&&'],
  $logic_or: ['or', '||'],
  $logic_not: ['not', '!'],
  $_AND_: ['and', null],
  $_OR_: ['or', null],
  $_XOR_: ['xor', null],
  $_NAND_: ['nand', null],
  $_NOR_: ['nor', null],
  $_XNOR_: ['xnor', null],
  $_NOT_: ['not', null],
  $_BUF_: ['buf', null],
};

const ARITH: Record<string, string> = {
  $add: '+',
  $sub: '−',
  $mul: '×',
  $div: '÷',
  $mod: '%',
  $divfloor: '÷',
  $modfloor: '%',
  $pow: '**',
  $neg: '−',
  $pos: '+',
  $shl: '<<',
  $shr: '>>',
  $sshl: '<<<',
  $sshr: '>>>',
  $shift: '>>',
  $shiftx: '>>',
  $alu: 'ALU',
  $macc: 'MAC',
  $fa: 'FA',
  $lcu: 'LCU',
};

const COMPARE: Record<string, string> = {
  $lt: '<',
  $le: '≤',
  $gt: '>',
  $ge: '≥',
  $eq: '=',
  $ne: '≠',
  $eqx: '≡',
  $nex: '≢',
};

/** Os registradores e travas, com a ordem dos pinos de entrada (de cima
 * para baixo; o relógio por último). */
const REG_PINS = ['D', 'AD', 'EN', 'E', 'CE', 'ALOAD', 'L', 'SET', 'S', 'CLR', 'R', 'SRST', 'ARST', 'CLK', 'C'];
const CLOCK_PINS = new Set(['CLK', 'C']);

export function isRegister(type: string): boolean {
  return (
    /^\$(a?l?dffe?|adffe?|sdffc?e?|dffsre?|a?dlatch(sr)?|sr|ff|aldffe?)$/.test(type) ||
    /^\$_(S?DFF|DFFE|DFFSR|SDFFC?E|ALDFF|DLATCH|SR)/.test(type)
  );
}

export function isMemory(type: string): boolean {
  return /^\$mem(rd|wr|init)?(_v2)?$/.test(type);
}

/** A família de uma célula; `module` quando o tipo é um módulo do netlist. */
export function cellCategory(type: string, isModule: boolean): Family {
  if (isModule) return 'module';
  if (type in GATES) return 'logic';
  if (type in ARITH) return 'arith';
  if (type in COMPARE) return 'compare';
  if (/^\$(_MUX\d*_|mux|pmux|bmux|bwmux|demux|_NMUX_)$/.test(type)) return 'mux';
  if (isRegister(type)) return 'reg';
  if (isMemory(type)) return 'mem';
  if (type === '$tribuf' || type === '$_TBUF_') return 'logic';
  return 'other';
}

/** O nome curto de um tipo genérico, para caixas e dicas: `$memrd_v2` → `memrd`. */
export function typeLabel(type: string): string {
  return type.replace(/^\$_?/, '').replace(/_v2$/, '').replace(/_$/, '');
}

/** Os nomes que o PRISM dá às células de memória e às sem desenho próprio. */
const BOX_TITLES: Record<string, string> = {
  $memrd: 'mem read',
  $memwr: 'mem write',
  $meminit: 'mem init',
  $mem: 'memory',
};

export function boxTitle(type: string): string {
  return BOX_TITLES[type.replace(/_v2$/, '')] ?? typeLabel(type);
}

// Símbolos --------------------------------------------------------------------

export interface CellPorts {
  inputs: string[];
  outputs: string[];
}

/** O símbolo de uma célula primitiva, ou `box` quando ela não tem desenho
 * próprio (ou tem portas que o desenho próprio não prevê). */
export function cellShape(type: string, ports: CellPorts, parameters: Record<string, string> = {}): Shape {
  const has = (inputs: string[], outputs: string[]) =>
    ports.inputs.length === inputs.length &&
    inputs.every((name) => ports.inputs.includes(name)) &&
    ports.outputs.length === outputs.length &&
    outputs.every((name) => ports.outputs.includes(name));
  const gate = GATES[type];
  if (gate) {
    const unary = gate[0] === 'not' || gate[0] === 'buf' || type.startsWith('$reduce_');
    if (unary ? has(['A'], ['Y']) : has(['A', 'B'], ['Y'])) return { kind: 'gate', gate: gate[0], glyph: gate[1] };
  }
  const glyph = ARITH[type] ?? COMPARE[type];
  if (glyph && (has(['A', 'B'], ['Y']) || has(['A'], ['Y']))) return { kind: 'op', glyph };
  if ((type === '$mux' || type === '$_MUX_') && has(['A', 'B', 'S'], ['Y'])) return { kind: 'mux', parallel: false };
  if (type === '$pmux' && has(['A', 'B', 'S'], ['Y'])) return { kind: 'mux', parallel: true };
  if (isRegister(type) && ports.outputs.length === 1 && ports.inputs.every((name) => REG_PINS.includes(name))) {
    // Nas de porta lógica, a primeira letra depois do nome é a polaridade
    // do relógio ($_DFF_N_, $_DFFE_PN_).
    const polarity = parameters.CLK_POLARITY ?? parameters.EN_POLARITY;
    const negative = type.startsWith('$_') ? /^\$_[A-Z]+_N/.test(type) : polarity === '0';
    return {
      kind: 'reg',
      title: type.startsWith('$_') ? 'FF' : typeLabel(type).toUpperCase(),
      clockInverted: negative,
    };
  }
  return { kind: 'box' };
}

// Geometria -------------------------------------------------------------------

export function portNodeGeometry(kind: 'in' | 'out' | 'inout', name: string): Geometry {
  const width = round4(textWidth(name) + 26);
  const height = 20;
  const port: PortGeometry =
    kind === 'out'
      ? { name, side: 'W', x: 0, y: height / 2, label: null, output: false }
      : { name, side: 'E', x: width, y: height / 2, label: null, output: true };
  return { width, height, ports: [port] };
}

/** Uma etiqueta de porta (o rótulo de uma rede global ou uma constante),
 * escrita ao lado da porta no lugar do fio. */
export const TAG_HEIGHT = 13;
export const tagWidth = (text: string) => round4(textWidth(text, 9.5) + 16);

export function gateGeometry(shape: Extract<Shape, { kind: 'gate' }>, unary: boolean): Geometry {
  if (shape.gate === 'not' || shape.gate === 'buf') {
    const width = shape.glyph ? 40 : 36;
    return {
      width,
      height: 24,
      ports: [
        { name: 'A', side: 'W', x: 0, y: 12, label: null, output: false },
        { name: 'Y', side: 'E', x: width, y: 12, label: null, output: true },
      ],
    };
  }
  const inputs: PortGeometry[] = unary
    ? [{ name: 'A', side: 'W', x: 0, y: 16, label: null, output: false }]
    : [
        { name: 'A', side: 'W', x: 0, y: 8, label: null, output: false },
        { name: 'B', side: 'W', x: 0, y: 24, label: null, output: false },
      ];
  return { width: 44, height: 32, ports: [...inputs, { name: 'Y', side: 'E', x: 44, y: 16, label: null, output: true }] };
}

export function opGeometry(glyph: string, unary: boolean): Geometry {
  const size = Math.max(32, round4(textWidth(glyph, FONT.glyph) + 14));
  const inputs: PortGeometry[] = unary
    ? [{ name: 'A', side: 'W', x: 0, y: 16, label: null, output: false }]
    : [
        { name: 'A', side: 'W', x: 0, y: 8, label: null, output: false },
        { name: 'B', side: 'W', x: 0, y: 24, label: null, output: false },
      ];
  return { width: size, height: 32, ports: [...inputs, { name: 'Y', side: 'E', x: size, y: 16, label: null, output: true }] };
}

export function muxGeometry(parallel: boolean): Geometry {
  const width = 28;
  const height = parallel ? 52 : 44;
  const [a, b] = parallel ? [16, 36] : [14, 30];
  return {
    width,
    height,
    ports: [
      { name: 'A', side: 'W', x: 0, y: a, label: parallel ? 'A' : '0', output: false },
      { name: 'B', side: 'W', x: 0, y: b, label: parallel ? 'B' : '1', output: false },
      { name: 'S', side: 'S', x: width / 2, y: height, label: null, output: false },
      { name: 'Y', side: 'E', x: width, y: height / 2, label: null, output: true },
    ],
  };
}

export function regGeometry(ports: CellPorts): Geometry {
  const inputs = [...ports.inputs].sort((a, b) => REG_PINS.indexOf(a) - REG_PINS.indexOf(b));
  const top = 18;
  const height = top + PITCH * Math.max(2, inputs.length) - 2;
  const widest = Math.max(...inputs.filter((name) => !CLOCK_PINS.has(name)).map((name) => textWidth(name)), 0);
  const width = round4(Math.max(52, widest + textWidth(ports.outputs[0] ?? 'Q') + 28));
  const geometry: PortGeometry[] = inputs.map((name, i) => {
    // O relógio fica embaixo, mesmo com só D e CLK.
    const row = CLOCK_PINS.has(name) ? Math.max(1, inputs.length - 1) : i;
    return { name, side: 'W', x: 0, y: top + PITCH * row + 4, label: CLOCK_PINS.has(name) ? null : name, output: false };
  });
  geometry.push({ name: ports.outputs[0] ?? 'Q', side: 'E', x: width, y: top + 4, label: ports.outputs[0] ?? 'Q', output: true });
  return { width, height, ports: geometry };
}

/**
 * Uma caixa com título (e subtítulo), as entradas à esquerda e as saídas à
 * direita, cada uma com o nome: instâncias de módulo, memórias e as células
 * sem desenho próprio. `icon`: o título divide a linha com o ícone de entrar
 * no submódulo.
 */
export function boxGeometry(title: string, subtitle: string | null, ports: CellPorts, icon = false): Geometry {
  const header = subtitle ? 36 : 24;
  const rows = Math.max(ports.inputs.length, ports.outputs.length, 1);
  const inWidth = Math.max(0, ...ports.inputs.map((name) => textWidth(name)));
  const outWidth = Math.max(0, ...ports.outputs.map((name) => textWidth(name)));
  const width = round4(
    Math.max(
      72,
      textWidth(title, FONT.title) + (icon ? 44 : 24),
      subtitle ? textWidth(subtitle) + 24 : 0,
      inWidth + outWidth + (inWidth && outWidth ? 32 : 20),
    ),
  );
  const height = header + PITCH * rows + 4;
  const y = (i: number) => header + PITCH * i + PITCH / 2 + 2;
  return {
    width,
    height,
    ports: [
      ...ports.inputs.map((name, i): PortGeometry => ({ name, side: 'W', x: 0, y: y(i), label: name, output: false })),
      ...ports.outputs.map((name, i): PortGeometry => ({ name, side: 'E', x: width, y: y(i), label: name, output: true })),
    ],
  };
}

/** A barra que separa um barramento em fatias (`split`) ou junta fatias num
 * barramento (`join`). `labels` são as faixas de bits do lado das fatias. */
export function sliceGeometry(kind: 'split' | 'join', labels: string[]): Geometry {
  const widest = Math.max(...labels.map((label) => textWidth(label, 9)), 8);
  const width = round4(widest + 16);
  const height = SLICE_PITCH * labels.length + 4;
  const y = (i: number) => SLICE_PITCH * i + SLICE_PITCH / 2 + 2;
  const many: PortGeometry[] = labels.map((label, i) =>
    kind === 'split'
      ? { name: `o${i}`, side: 'E', x: width, y: y(i), label, output: true }
      : { name: `i${i}`, side: 'W', x: 0, y: y(i), label, output: false },
  );
  const one: PortGeometry =
    kind === 'split'
      ? { name: 'i', side: 'W', x: 0, y: height / 2, label: null, output: false }
      : { name: 'o', side: 'E', x: width, y: height / 2, label: null, output: true };
  return { width, height, ports: kind === 'split' ? [one, ...many] : [...many, one] };
}
