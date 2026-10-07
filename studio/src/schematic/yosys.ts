// O netlist que a síntese grava (`write_json` do Yosys, em
// `.lace/Temp/synth/<topo>/hierarchy.json`) e os nomes legíveis que saem
// dele. Nada aqui depende do DOM: o PRISM e os testes usam o mesmo código.
//
// Os bits de um sinal vêm do LSB para o MSB. Um bit é o número de um fio do
// módulo ou uma constante ('0', '1', 'x', 'z').

export type Bit = number | '0' | '1' | 'x' | 'z';
export type Direction = 'input' | 'output' | 'inout';

export interface YosysPort {
  direction: Direction;
  bits: Bit[];
  offset?: number;
  upto?: number;
  signed?: number;
}

export interface YosysCell {
  hide_name: number;
  type: string;
  parameters?: Record<string, string>;
  attributes?: Record<string, string>;
  port_directions?: Record<string, Direction>;
  connections: Record<string, Bit[]>;
}

export interface YosysNet {
  hide_name: number;
  bits: Bit[];
  attributes?: Record<string, string>;
  offset?: number;
  upto?: number;
  signed?: number;
}

export interface YosysModule {
  attributes?: Record<string, string>;
  parameter_default_values?: Record<string, string>;
  ports?: Record<string, YosysPort>;
  cells?: Record<string, YosysCell>;
  netnames?: Record<string, YosysNet>;
}

export interface Netlist {
  creator?: string;
  modules: Record<string, YosysModule>;
}

/** Lê o texto do `write_json`. Só confere a forma de cima: o resto o
 * Yosys garante. */
export function parseNetlist(text: string): Netlist {
  const value = JSON.parse(text) as Partial<Netlist>;
  if (!value || typeof value !== 'object' || !value.modules || typeof value.modules !== 'object') {
    throw new Error('not a Yosys JSON netlist (no "modules")');
  }
  return value as Netlist;
}

/** O módulo marcado como topo; sem marca, o primeiro que ninguém instancia. */
export function topModule(netlist: Netlist): string | null {
  const names = Object.keys(netlist.modules);
  const marked = names.find((name) => isTrue(netlist.modules[name]!.attributes?.top));
  if (marked) return marked;
  const used = new Set<string>();
  for (const module of Object.values(netlist.modules)) {
    for (const cell of Object.values(module.cells ?? {})) used.add(cell.type);
  }
  return names.find((name) => !used.has(name) && !isBlackbox(netlist.modules[name]!)) ?? names[0] ?? null;
}

export function isBlackbox(module: YosysModule): boolean {
  return isTrue(module.attributes?.blackbox) || isTrue(module.attributes?.whitebox);
}

/** Um atributo ou parâmetro booleano do Yosys: "1" ou o binário de 32 bits. */
export function isTrue(value: string | undefined): boolean {
  return !!value && /^[01]+$/.test(value) && value.includes('1');
}

// Nomes ---------------------------------------------------------------------

/** O nome de um objeto do Yosys sem a barra dos nomes públicos (`\clk`). */
export function plainName(name: string): string {
  return name.startsWith('\\') ? name.slice(1) : name;
}

/**
 * O nome de um módulo como o usuário o escreveu. Um módulo derivado de
 * parâmetros chega como `$paramod\pc\NBITS=s32'...` ou, com parâmetros
 * longos, `$paramod$<hash>\fir_tap`; o `hdlname` que o Yosys grava no
 * módulo vale mais que os dois.
 */
export function moduleDisplayName(name: string, module?: YosysModule): string {
  const hdl = module?.attributes?.hdlname;
  if (hdl) return hdl.split(' ').pop()!;
  if (name.startsWith('$paramod')) {
    const rest = name.slice('$paramod'.length);
    const parts = rest.split('\\').filter(Boolean);
    // `$<hash>` antes do nome, ou o nome seguido de `P=V`.
    const base = parts.find((part) => !part.startsWith('$') && !part.includes('='));
    if (base) return base;
  }
  return plainName(name);
}

/** Os parâmetros com que o módulo foi derivado, já em decimal quando dá. */
export function moduleParameters(name: string, module?: YosysModule): [string, string][] {
  const values = module?.parameter_default_values;
  if (values && Object.keys(values).length) {
    return Object.entries(values).map(([key, value]) => [key, parameterValue(value)]);
  }
  if (!name.startsWith('$paramod\\')) return [];
  return name
    .split('\\')
    .slice(2)
    .map((part) => part.split('='))
    .filter((pair) => pair.length === 2)
    .map(([key, value]) => [key!, encodedParameter(value!)]);
}

/** `s32'00000000000000000000000000001000` (nome de `$paramod`) → `8`. */
function encodedParameter(value: string): string {
  const match = /^(s?)(\d+)'([01xz]+)$/.exec(value);
  if (!match) return value;
  return parameterValue(match[3]!, match[1] === 's');
}

/** Um parâmetro do Yosys em texto: binário vira decimal (com sinal se
 * pedido), binário com x/z fica como está, texto fica como texto. */
export function parameterValue(value: string, signed = false): string {
  if (!/^[01xz]+$/.test(value)) return value.trimEnd();
  if (/[xz]/.test(value)) return `${value.length}'b${value}`;
  if (value.length > 64) return `${value.length}'h${BigInt(`0b${value}`).toString(16)}`;
  let number = BigInt(`0b${value}`);
  if (signed && value[0] === '1') number -= 1n << BigInt(value.length);
  return number.toString();
}

/** Um parâmetro de célula como número (larguras, polaridades). */
export function parameterNumber(cell: YosysCell, key: string): number | null {
  const value = cell.parameters?.[key];
  if (value === undefined || !/^[01]+$/.test(value)) return null;
  return Number.parseInt(value, 2);
}

/** Uma constante (bits do LSB para o MSB) como o Verilog escreveria. */
export function constantText(bits: Bit[]): string {
  const msbFirst = bits.map(String).reverse().join('');
  if (bits.length === 1) return msbFirst;
  if (/[xz]/.test(msbFirst) || bits.length <= 4) return `${bits.length}'b${msbFirst}`;
  const hex = BigInt(`0b${msbFirst}`).toString(16).toUpperCase();
  return `${bits.length}'h${hex}`;
}

// Fonte -----------------------------------------------------------------------

export interface SourceRef {
  file: string;
  line: number;
  column: number;
}

/**
 * O atributo `src` do Yosys: `arquivo:linha.coluna-linha.coluna`, às vezes
 * vários separados por `|` (uma célula de `proc` aponta para o `if` e para o
 * bloco). O arquivo pode ter `:` (`C:/...`), então a conta é da direita.
 */
export function parseSource(src: string | undefined): SourceRef[] {
  if (!src) return [];
  const refs: SourceRef[] = [];
  for (const part of src.split('|')) {
    const match = /^(.*):(\d+)(?:\.(\d+))?(?:-\d+(?:\.\d+)?)?$/.exec(part);
    if (!match || !match[1]) continue;
    refs.push({ file: match[1], line: Number(match[2]), column: Number(match[3] ?? 1) });
  }
  return refs;
}
