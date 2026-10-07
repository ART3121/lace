// Os tipos do próprio Studio, espelhando os structs Rust de src-tauri/src.
// Os tipos do Core do Lace (BuildResult, SimulationResult, Event...) vêm
// gerados em ./lace-types.ts. A referência de cada comando está em
// docs/IPC.md.
//
// Ao mudar um struct em Rust, mude aqui também: não há geração automática
// destes (ver docs/ARCHITECTURE.md, "Contrato entre backend e interface").

import type {
  AddedFile,
  BuildResult,
  Elaboration,
  HierarchyResult,
  ModuleInstance,
  MovedPath,
  BundleComponent,
  CheckResult,
  Event,
  FileRole,
  Language,
  Processor,
  ProjectIssue,
  SchematicResult,
  SimulationResult,
  Simulator,
  SynthesisResult,
  SystemCompiler,
  WaveProcessor,
} from './lace-types';

export type {
  AddedFile,
  Elaboration,
  HierarchyResult,
  Language,
  ModuleInstance,
  MovedPath,
  Processor,
  ProjectIssue,
  Simulator,
  WaveProcessor,
};

/** `error.rs`: o erro de todo comando. */
export interface IpcError {
  code: string;
  message: string;
}

/** `commands/app.rs`: AppInfo. */
export interface AppInfo {
  name: string;
  version: string;
  os: string;
  lace_platform: string | null;
  config_dir: string | null;
  tauri_version: string;
  webview_version: string | null;
  /** Build de desenvolvimento. */
  debug: boolean;
}

/** `settings.rs`: Settings. */
export interface Settings {
  language: 'system' | 'pt' | 'en';
  /** O id de um tema de themes/catalog.ts, ou `system`. */
  theme: string;
  toolchain_dir: string | null;
  compiler_dir: string | null;
  simulator: Simulator;
  open_wave_after_sim: boolean;
  /** Onde a onda abre: numa aba (cliente web do Surfer) ou em janela. */
  wave_viewer: 'tab' | 'window';
  sim_timeout_s: number | null;
  verbose: boolean;
  restore_last_project: boolean;
  /** Windows: `powershell` ou `cmd`. */
  terminal_shell: 'powershell' | 'cmd';
  editor: EditorSettings;
  zen: ZenSettings;
  layouts: LayoutSettings;
  recent_projects: RecentProject[];
}

/** `settings.rs`: LayoutSettings. Cada layout gravado vai como a interface o
 * escreveu (state/layoutModel.ts); o backend não o lê. */
export interface LayoutSettings {
  /** O id do layout em uso: um dos gravados ou um pronto (`default`). */
  active: string;
  saved: unknown[];
}

/** `settings.rs`: ZenSettings. */
export interface ZenSettings {
  fullscreen: boolean;
  center_layout: boolean;
  show_tabs: boolean;
  hide_line_numbers: boolean;
}

export interface EditorSettings {
  font_size: number;
  tab_size: number;
  word_wrap: boolean;
  minimap: boolean;
  auto_save: boolean;
  vim_mode: boolean;
}

export interface RecentProject {
  spf: string;
  name: string;
  opened_at_ms: number;
}

/** `commands/app.rs`: RecentStatus. */
export interface RecentStatus extends RecentProject {
  exists: boolean;
}

/** `commands/project.rs`: ProjectFile do Core. */
export interface ProjectFile {
  role: FileRole;
  path: string;
  top_level: boolean;
}

/** `commands/project.rs`: ProjectSnapshot. */
export interface ProjectSnapshot {
  name: string;
  spf: string;
  root: string;
  synthesizable: ProjectFile[];
  testbenches: ProjectFile[];
  top_level: string | null;
  top_module: string | null;
  top_module_error: IpcError | null;
  selected_testbench: string | null;
  testbench_module: string | null;
  unregistered: string[];
  processors: ProcessorStatus[];
  waveform: string | null;
  /** Os Verilog que podem ser o topo (a regra do Core: qualquer um, menos
   * nome de testbench), registrados, gerados e de fora do `.spf`. */
  top_candidates: string[];
  /** O que está estranho no `.spf` sem impedir de abrir (`Project::issues`). */
  issues: ProjectIssue[];
}

export interface PortFile {
  port: number;
  path: string;
}

/** `commands/project.rs`: ProcessorStatus (o Processor do Core achatado). */
export interface ProcessorStatus extends Processor {
  built: boolean;
  inputs: PortFile[];
  outputs: PortFile[];
  missing_inputs: string[];
  waveform: string | null;
  generated: GeneratedFiles;
}

/** `commands/project.rs`: GeneratedFiles. Só os que existem. */
export interface GeneratedFiles {
  verilog: string | null;
  testbench: string | null;
  assembly: string | null;
  memories: string[];
  intermediates: string[];
}

/** `commands/history.rs`: CleanupRequest. */
export type CleanupRequest = { kind: 'all' } | { kind: 'keep_latest'; keep: number } | { kind: 'reports'; ids: string[] };

/** `commands/history.rs`: CleanReport. */
export interface CleanReport {
  removed: string[];
  kept: number;
}

/** `commands/project.rs`: NewProcessorRequest. */
export interface NewProcessorRequest {
  name: string;
  language: Language;
  input_ports?: number;
  output_ports?: number;
  nubits?: number;
  nbmant?: number;
  nbexpo?: number;
  nugain?: number;
  ndstac?: number;
  sdepth?: number;
}

/** O NewProcessor do Core, com os padrões da AURORA. */
export interface NewProcessorDefaults {
  name: string;
  language: Language;
  nubits: number;
  nbmant: number;
  nbexpo: number;
  nugain: number;
  ndstac: number;
  sdepth: number;
  input_ports: number;
  output_ports: number;
}

/** `commands/files.rs`. */
export interface DirEntry {
  name: string;
  path: string;
  is_dir: boolean;
  hidden: boolean;
  /** Nome de testbench (`tb_<nome>.v`, `<nome>_tb.v`): não pode ser o topo. */
  testbench_name: boolean;
}

export interface TextFile {
  path: string;
  content: string;
  modified_ms: number;
  binary: boolean;
  too_large: boolean;
}

export interface FileStat {
  is_dir: boolean;
  modified_ms: number;
  size: number;
}

export interface SearchMatch {
  path: string;
  line: number;
  column: number;
  length: number;
  text: string;
}

/** `flows.rs`: FlowRequest. */
export type FlowRequest =
  | { flow: 'build'; processors?: string[] }
  | { flow: 'check'; file?: string | null; processor?: string | null; lint?: boolean }
  | {
      flow: 'simulate';
      processor?: string | null;
      testbench?: string | null;
      simulator: Simulator;
      /** A simulação rápida: sem onda, com o testbench Verilog no
       * Verilator; `simulator` só vale para um testbench cocotb. */
      fast?: boolean;
      timeout_s?: number | null;
      open_wave?: boolean;
    }
  | { flow: 'synthesize'; processor?: string | null; schematic?: boolean; module?: string | null }
  | { flow: 'schematic'; netlist: string; module: string; bus_widths?: boolean };

export type FlowName = FlowRequest['flow'];

/** `flows.rs`: PortValues. */
export interface PortValues {
  port: number;
  path: string;
  values: number[];
  error: IpcError | null;
}

/** `flows.rs`: WaveOpened. */
export interface WaveOpened {
  waveform: string;
  pid: number;
  log: string;
  /** Processadores compilados de novo depois da simulação: PC e linha em
   * números até simular de novo. */
  outdated: string[];
}

/** `flows.rs`: FlowOutcome. */
export interface FlowOutcome {
  flow: FlowName;
  command: string;
  succeeded: boolean;
  builds: BuildResult[];
  check: CheckResult | null;
  simulation: SimulationResult | null;
  outputs: PortValues[];
  synthesis: SynthesisResult | null;
  schematic: SchematicResult | null;
  wave: WaveOpened | null;
  /** A onda a abrir numa aba, no lugar da janela. */
  wave_tab: string | null;
  wave_error: IpcError | null;
  /** Por que o esquemático não saiu depois da síntese (grande demais). */
  schematic_error: IpcError | null;
  report: string | null;
  report_error: string | null;
}

/** As operações que rodam a CLI (`lace install`, `lace update`). */
export type CliFlow = 'install' | 'update';

/** O resultado de uma operação da CLI, embrulhado pelo Studio. `result` é o
 * JSON que ela escreveu: o relatório do comando, `{error}` quando falhou, ou
 * `null` quando não escreveu nada. */
export interface CliOutcome {
  flow: CliFlow;
  succeeded: boolean;
  cancelled: boolean;
  exit_code: number | null;
  result: unknown;
}

/** `flows.rs`: Phase. */
export type Phase = 'build' | 'check' | 'simulate' | 'synthesize' | 'schematic' | 'wave';

/** `jobs.rs`: JobMessage. */
export type JobMessage =
  | { type: 'started'; job: number; command: string }
  | { type: 'phase'; phase: Phase }
  | { type: 'events'; events: Event[] }
  | { type: 'build'; result: BuildResult }
  | { type: 'cli_output'; stream: 'stdout' | 'stderr'; line: string }
  | { type: 'finished'; outcome: FlowOutcome | CliOutcome }
  | { type: 'failed'; error: IpcError };

/** `toolchain.rs`. */
export type ToolchainOrigin = 'settings' | 'environment' | 'bundled' | 'installation' | 'path' | 'beside';

export interface ToolStatus {
  name: string;
  component: string | null;
  path: string | null;
  system: boolean;
  error: IpcError | null;
}

export interface ToolchainInfo {
  found: boolean;
  error: IpcError | null;
  origin: ToolchainOrigin | null;
  root: string | null;
  bundle: string | null;
  platform: string | null;
  components: BundleComponent[];
  not_installed: string[];
  tools: ToolStatus[];
  system_compiler: SystemCompiler | null;
  compiler_error: IpcError | null;
  lace_cli: string | null;
  /** O cliente web do Surfer do bundle (`surfer-aurora/web`), para a onda
   * numa aba; `null` num bundle que não o traz. */
  surfer_web: string | null;
}

/** `ListPosition` do Core (`Project::reorder_file`, `lace order`). */
export type ListPosition =
  | { kind: 'first' }
  | { kind: 'last' }
  | { kind: 'before'; path: string }
  | { kind: 'after'; path: string };

/** `wave_tab.rs`: WaveTab. */
export interface WaveTab {
  id: string;
  url: string;
  processors: WaveProcessor[];
}

/** `terminal.rs`: TerminalMessage. */
export type TerminalMessage = { type: 'data'; data: string } | { type: 'exit'; code: number | null };

/** `watcher.rs`: o evento `studio://fs-changed`. */
export interface FsChanged {
  paths: string[];
}
