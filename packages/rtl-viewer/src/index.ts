// O RTL viewer da AURORA: o esquemático interativo do netlist que o Yosys
// grava na síntese. Quem o abriga (o Studio, a extensão do VS Code, o HTML
// do `aurora synth --open`) cria o estado, configura o worker do layout e
// monta o componente; o CSS vem de rtl-viewer.css.

export { RtlViewer, type RtlViewerProps } from './RtlViewer';
export {
  createRtlViewerStore,
  type LoadedNetlist,
  type NetlistReader,
  type RtlViewerState,
  type RtlViewerStore,
  type ViewerOptions,
} from './store';
export { cancelLayouts, setLayoutWorker, workerFromSource } from './engine';
export { setLang, type Lang } from './i18n';
export type { RtlViewerUi, ViewerMenuItem, MenuEvent, ButtonProps, IconButtonProps } from './ui';
export type { SourceRef } from './yosys';
