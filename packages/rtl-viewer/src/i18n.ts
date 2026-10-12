// As traduções do RTL viewer: as mesmas chaves que ele tinha no Studio
// (`schematic.*`), agora dele. O idioma vem de quem abriga o viewer
// (`setLang`); sem tradução para uma chave, vale o português, como no Studio.

import { create } from 'zustand';

const pt = {
  'schematic.loading': 'Lendo o netlist...',
  'schematic.loadFailed': 'Não deu para ler o netlist: {message}',
  'schematic.noNetlist': 'Nenhum netlist para desenhar: sintetize primeiro.',
  'action.zoomIn': 'Aumentar o zoom',
  'action.zoomOut': 'Diminuir o zoom',
  'schematic.rendering': 'Desenhando {module}...',
  'schematic.cancel': 'Parar',
  'schematic.cancelled': 'Layout interrompido.',
  'schematic.layoutFailed': 'O layout falhou: {message}',
  'schematic.retry': 'Desenhar de novo',
  'schematic.large': 'Este módulo tem {cells} células; o layout pode levar um bom tempo.',
  'schematic.drawAnyway': 'Desenhar assim mesmo',
  'schematic.pathGone': 'Esta instância não existe mais no netlist.',
  'schematic.back': 'Voltar (Esc, Alt+←)',
  'schematic.forward': 'Avançar (Alt+→)',
  'schematic.search': 'Buscar célula, rede ou porta',
  'schematic.searchCount': '{count} achados',
  'schematic.searchPosition': '{index} de {count}',
  'schematic.noMatch': 'nada',
  'schematic.netNames': 'Nomes das redes',
  'schematic.flags': 'Redes globais como rótulo (clk, rst...)',
  'schematic.busWidths': 'Largura dos barramentos',
  'schematic.fit': 'Ajustar à janela',
  'schematic.actualSize': 'Tamanho real',
  'schematic.tree': 'Hierarquia',
  'schematic.topTag': 'topo',
  'schematic.inspector': 'Detalhes',
  'schematic.export': 'Exportar SVG',
  'schematic.resynthesize': 'Sintetizar de novo',
  'schematic.openModule': 'Abrir o módulo',
  'schematic.goToSource': 'Ir ao código',
  'schematic.highlight': 'Destacar as ligações',
  'schematic.copyName': 'Copiar o nome',
  'schematic.hint.enter': 'Dois cliques: abrir o módulo',
  'schematic.hint.source': 'Dois cliques: ir ao código',
  'schematic.stats': '{cells} células · {nets} redes · layout em {ms} ms',
  'schematic.bit': '1 bit',
  'schematic.bits': '{count} bits',
  'schematic.loadsCount': '{count} destinos',
  'schematic.kind.in': 'entrada',
  'schematic.kind.out': 'saída',
  'schematic.kind.inout': 'bidirecional',
  'schematic.kind.const': 'constante',
  'schematic.kind.undriven': 'sem driver',
  'schematic.kind.split': 'fatias de um barramento',
  'schematic.kind.flag': 'rótulo de rede global',
  'schematic.kind.join': 'concatenação',
  'schematic.kind.instance': 'instância de {module}',
  'schematic.net': 'Rede',
  'schematic.unnamed': 'sem nome',
  'schematic.unconnected': 'desligada',
  'schematic.driver': 'Origem',
  'schematic.loads': 'Destinos ({count})',
  'schematic.source': 'Código',
  'schematic.parameters': 'Parâmetros',
  'schematic.allParameters': 'Mostrar os {count}',
  'schematic.fewer': 'Mostrar menos',
  'schematic.ports': 'Portas',
  'schematic.cells': 'Células',
  'schematic.noCells': 'Nenhuma célula: o módulo só liga portas.',
  'schematic.instances': 'Instâncias',
  'schematic.moduleSummary': 'Módulo desenhado',
  'schematic.family.arith': 'Aritmética',
  'schematic.family.logic': 'Lógica',
  'schematic.family.compare': 'Comparação',
  'schematic.family.mux': 'Multiplexador',
  'schematic.family.reg': 'Registrador',
  'schematic.family.mem': 'Memória',
  'schematic.family.module': 'Submódulo',
  'schematic.family.other': 'Outras',
} as const;

export type Key = keyof typeof pt;
export type Lang = 'pt' | 'en';

const en: Record<Key, string> = {
  'schematic.loading': 'Reading the netlist...',
  'schematic.loadFailed': 'Could not read the netlist: {message}',
  'schematic.noNetlist': 'No netlist to draw: synthesize first.',
  'action.zoomIn': 'Zoom in',
  'action.zoomOut': 'Zoom out',
  'schematic.rendering': 'Drawing {module}...',
  'schematic.cancel': 'Stop',
  'schematic.cancelled': 'Layout stopped.',
  'schematic.layoutFailed': 'Layout failed: {message}',
  'schematic.retry': 'Draw again',
  'schematic.large': 'This module has {cells} cells; the layout may take a while.',
  'schematic.drawAnyway': 'Draw anyway',
  'schematic.pathGone': 'This instance is no longer in the netlist.',
  'schematic.back': 'Back (Esc, Alt+←)',
  'schematic.forward': 'Forward (Alt+→)',
  'schematic.search': 'Find a cell, net or port',
  'schematic.searchCount': '{count} found',
  'schematic.searchPosition': '{index} of {count}',
  'schematic.noMatch': 'none',
  'schematic.netNames': 'Net names',
  'schematic.flags': 'Global nets as labels (clk, rst...)',
  'schematic.busWidths': 'Bus widths',
  'schematic.fit': 'Fit to window',
  'schematic.actualSize': 'Actual size',
  'schematic.tree': 'Hierarchy',
  'schematic.topTag': 'top',
  'schematic.inspector': 'Details',
  'schematic.export': 'Export SVG',
  'schematic.resynthesize': 'Synthesize again',
  'schematic.openModule': 'Open the module',
  'schematic.goToSource': 'Go to source',
  'schematic.highlight': 'Highlight connections',
  'schematic.copyName': 'Copy name',
  'schematic.hint.enter': 'Double-click: open the module',
  'schematic.hint.source': 'Double-click: go to source',
  'schematic.stats': '{cells} cells · {nets} nets · layout in {ms} ms',
  'schematic.bit': '1 bit',
  'schematic.bits': '{count} bits',
  'schematic.loadsCount': '{count} loads',
  'schematic.kind.in': 'input',
  'schematic.kind.out': 'output',
  'schematic.kind.inout': 'inout',
  'schematic.kind.const': 'constant',
  'schematic.kind.undriven': 'undriven',
  'schematic.kind.split': 'bus slices',
  'schematic.kind.flag': 'global net label',
  'schematic.kind.join': 'concatenation',
  'schematic.kind.instance': 'instance of {module}',
  'schematic.net': 'Net',
  'schematic.unnamed': 'unnamed',
  'schematic.unconnected': 'unconnected',
  'schematic.driver': 'Driver',
  'schematic.loads': 'Loads ({count})',
  'schematic.source': 'Source',
  'schematic.parameters': 'Parameters',
  'schematic.allParameters': 'Show all {count}',
  'schematic.fewer': 'Show fewer',
  'schematic.ports': 'Ports',
  'schematic.cells': 'Cells',
  'schematic.noCells': 'No cells: the module only connects ports.',
  'schematic.instances': 'Instances',
  'schematic.moduleSummary': 'Module on screen',
  'schematic.family.arith': 'Arithmetic',
  'schematic.family.logic': 'Logic',
  'schematic.family.compare': 'Compare',
  'schematic.family.mux': 'Multiplexer',
  'schematic.family.reg': 'Register',
  'schematic.family.mem': 'Memory',
  'schematic.family.module': 'Submodule',
  'schematic.family.other': 'Other',
};

const dictionaries: Record<Lang, Record<Key, string>> = { pt, en };

const useLang = create<{ lang: Lang }>(() => ({ lang: 'pt' }));

/** O idioma do viewer. Quem abriga chama ao montar e quando o idioma muda. */
export function setLang(lang: Lang): void {
  if (useLang.getState().lang !== lang) useLang.setState({ lang });
}

export type Params = Record<string, string | number>;

/** O texto de `key` no idioma atual, com os `{placeholders}` trocados. */
export function t(key: Key, params?: Params): string {
  const text = dictionaries[useLang.getState().lang][key] ?? pt[key] ?? key;
  if (!params) return text;
  return text.replace(/\{(\w+)\}/g, (match, name: string) => (name in params ? String(params[name]) : match));
}

/** `t` para componentes: redesenha quando o idioma muda. */
export function useT(): typeof t {
  useLang((s) => s.lang);
  return t;
}
