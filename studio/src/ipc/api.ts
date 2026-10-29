// As chamadas ao backend, com os tipos de entrada e saída. O resto da
// interface só fala com o backend por aqui. Cada função chama um comando
// Tauri do Studio ou, por `lace()`, um método do protocolo do Lace, que o
// backend repassa ao servidor (`aurora serve`). Referência: docs/IPC.md.

import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AddedFile,
  Board,
  Cleanup,
  ExperimentalReport,
  Feature,
  FileMismatch,
  NameCheck,
  ProcRemoveReport,
  RemoveOutcome,
  TopOutcome,
  WaveChoiceOutcome,
  ProjectFile,
  RunComparison,
  RunRecord,
  RunSummary,
  WaveSignals,
} from './lace-types';
import type {
  AppInfo,
  CleanReport,
  CleanupRequest,
  DirEntry,
  FileStat,
  FlowRequest,
  BitstreamState,
  FpgaConfig,
  FpgaPrepared,
  FsChanged,
  HierarchyResult,
  IpcError,
  JobMessage,
  LearnSnapshot,
  LearnTracks,
  ListPosition,
  ModuleInterface,
  MovedPath,
  LayoutConversion,
  NewProcessorDefaults,
  NewProcessorRequest,
  Processor,
  ProjectSnapshot,
  RecentStatus,
  SearchMatch,
  Settings,
  AgentEvent,
  AssistantInfo,
  TerminalMessage,
  TextFile,
  ToolchainInfo,
  WaveOpened,
  WaveTab,
} from './types';

/** Normaliza o que um `invoke` rejeitou para um `IpcError`. */
export function toIpcError(error: unknown): IpcError {
  if (error && typeof error === 'object' && 'code' in error && 'message' in error) {
    return error as IpcError;
  }
  return { code: 'internal', message: String(error) };
}

async function call<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toIpcError(error);
  }
}

/** Um método do protocolo do Lace, pelo servidor do Studio (`server.rs`):
 * o mesmo pedido e a mesma resposta do `aurora serve` (docs/PROTOCOL.md). Sem
 * `project` nos parâmetros, vale o projeto aberto. */
function lace<T>(method: string, params?: Record<string, unknown>): Promise<T> {
  return call<T>('lace_call', { method, params: params ?? null });
}

/** O `NameCheck` do protocolo como a interface o usa: `null` se o nome
 * serve, senão o erro. */
function nameError(check: NameCheck): IpcError | null {
  if (check.ok) return null;
  return { code: check.error?.code ?? 'invalid_name', message: check.error?.message ?? '' };
}

/** A limpeza do histórico como a interface a escreve, no formato do protocolo. */
function cleanupOf(request: CleanupRequest): Cleanup {
  switch (request.kind) {
    case 'all':
      return { kind: 'all' };
    case 'keep_latest':
      return { kind: 'keep_latest', value: request.keep };
    case 'reports':
      return { kind: 'reports', value: request.ids };
  }
}

export interface ReportView {
  id: string;
  path: string;
  record: RunRecord;
  text: string;
}

export const api = {
  app: {
    info: () => call<AppInfo>('app_info'),
    settings: () => call<Settings>('settings_get'),
    saveSettings: (settings: Settings) => call<Settings>('settings_set', { settings }),
    recent: () => call<RecentStatus[]>('recent_projects'),
    forgetRecent: (spf: string) => call<void>('recent_forget', { spf }),
    /** Abre uma onda no surfer-aurora: com `path`, aquele arquivo (a onda de
     * um exercício do `aurora learn` vem com o layout da correção); sem, a da
     * simulação do processador ou, sem processador, a do projeto. Um arquivo
     * vai com `project: null`, para não esperar na fila do projeto aberto. */
    openWave: (processor?: string | null, path?: string | null) =>
      lace<WaveOpened>(
        'wave.open',
        path
          ? { project: null, request: { source: { kind: 'file', path } } }
          : { request: { source: { kind: 'project', processor: processor ?? null } } },
      ),
    /** Fecha o Studio e, com `restart`, abre ele de novo (depois do `lace
     * update`). Quem chama grava os arquivos antes. */
    exit: (restart: boolean) => call<void>('app_exit', { restart }),
    /** Fecha o splash, depois de a janela principal aparecer. */
    closeSplash: () => call<void>('splash_close'),
  },

  /** A onda numa aba (`wave_tab.rs`). */
  waveTab: {
    open: (path: string) => call<WaveTab>('wave_tab_open', { path }),
    close: (id: string) => call<void>('wave_tab_close', { id }),
  },

  wave: {
    /** A árvore de sinais do testbench do projeto, com a escolha gravada. */
    signals: () => lace<WaveSignals>('wave.signals'),
    /** Grava a escolha (vazia: todos os sinais) e devolve como ficou. Os
     * caminhos são conferidos na árvore do testbench elaborado. */
    setSelection: (signals: string[]) =>
      lace<WaveChoiceOutcome>('wave.choose', { choice: { kind: 'set', paths: signals } }).then(
        (outcome) => outcome.selection,
      ),
    /** Apaga o layout salvo da onda; `false` se não havia. */
    resetLayout: (waveform: string) => lace<boolean>('wave.reset_layout', { waveform }),
  },

  project: {
    defaultParent: () => call<string>('project_default_parent'),
    open: (path: string) => call<ProjectSnapshot>('project_open', { path }),
    create: (parent: string, name: string) =>
      call<ProjectSnapshot>('project_create', { parent, name }),
    close: () => call<void>('project_close'),
    snapshot: () => lace<ProjectSnapshot>('project.status'),
    addVerilog: (path: string, testbench: boolean) =>
      lace<AddedFile[]>('project.add', { files: [path], testbench }).then((added) => added[0]),
    /** `false` se o arquivo não estava no projeto. */
    removeVerilog: (path: string) =>
      lace<RemoveOutcome>('project.remove', { files: [path] }).then((r) => r.removed.length > 0),
    /** Escolhe o topo por arquivo ou por módulo; devolve o arquivo de topo. */
    setTop: (target: string) =>
      lace<TopOutcome>('project.top', { target }).then((r) => r.top_level ?? ''),
    setTestbench: (path: string) => lace<ProjectSnapshot>('project.testbench', { path }),
    processorDefaults: () => lace<NewProcessorDefaults>('processor.defaults'),
    /** Campo ausente fica com o padrão da AURORA; sem `nubits`, com
     * `nbmant` ou `nbexpo` dados, ele vira `nbmant + nbexpo + 1`. */
    addProcessor: (request: NewProcessorRequest) => lace<Processor>('processor.add', { spec: request }),
    /** Tira o processador do projeto (`aurora proc remove`); com `toTrash`, a
     * pasta dele vai para a lixeira, e sem, fica no disco. */
    removeProcessor: (name: string, toTrash: boolean) =>
      lace<ProcRemoveReport>('processor.remove', { name, to_trash: toTrash }),
    configureProcessor: (
      name: string,
      config: { frequency_mhz?: number | null; clocks?: number | null; show_arrays?: boolean | null },
    ) =>
      lace<Processor>('processor.set', {
        name,
        config: {
          frequency_mhz: config.frequency_mhz ?? null,
          clocks: config.clocks ?? null,
          show_arrays: config.show_arrays ?? null,
        },
      }),
    outputValues: (processor: string, port: number) =>
      lace<number[]>('processor.outputs', { processor, port }),
    verilogModules: (text: string) => lace<string[]>('verilog.modules', { text }),
    /** Move ou renomeia (`Project::move_path` do Core): `to` é o caminho
     * final. Erro `path_exists` se ele já existe, sem `overwrite`; com, o
     * destino vai para a lixeira, menos o que o Lace mantém no lugar
     * (`cannot_replace`). */
    move: (from: string, to: string, overwrite = false) =>
      lace<MovedPath>('project.move', { from, to, overwrite }),
    /** Converte da estrutura de pastas do YANC 6.x para a do 7.1
     * (`Project::convert_layout`). */
    convertLayout: () => lace<LayoutConversion>('project.convert'),
    /** `null` se o nome de projeto serve; senão o erro do Core. */
    checkName: (name: string) => lace<NameCheck>('project.check_name', { name }).then(nameError),
    /** Muda a posição de um arquivo na lista dele (`aurora order`); devolve
     * a lista na ordem nova. */
    reorder: (path: string, position: ListPosition) =>
      lace<ProjectFile[]>('project.order', { file: path, position }),
    /** `null` se o nome de processador serve; senão o erro do Core. */
    checkProcessorName: (name: string) =>
      lace<NameCheck>('processor.check_name', { name }).then(nameError),
    hierarchy: () => call<HierarchyResult>('project_hierarchy'),
  },

  fs: {
    readDir: (path: string) => call<DirEntry[]>('fs_read_dir', { path }),
    readText: (path: string) => call<TextFile>('fs_read_text', { path }),
    writeText: (path: string, content: string, expectedModifiedMs?: number | null) =>
      call<number>('fs_write_text', {
        path,
        content,
        expectedModifiedMs: expectedModifiedMs ?? null,
      }),
    stat: (path: string) => call<FileStat | null>('fs_stat', { path }),
    createFile: (path: string, content?: string) =>
      call<void>('fs_create_file', { path, content: content ?? null }),
    createDir: (path: string) => call<void>('fs_create_dir', { path }),
    rename: (from: string, to: string) => call<void>('fs_rename', { from, to }),
    /** Copia um arquivo ou pasta (de qualquer lugar) para dentro de `toDir`. */
    copy: (from: string, toDir: string, overwrite = false) =>
      call<string>('fs_copy', { from, toDir, overwrite }),
    trash: (path: string) => call<void>('fs_trash', { path }),
    listFiles: () => call<string[]>('fs_list_files'),
    search: (query: string, regex: boolean, caseSensitive: boolean, maxResults?: number) =>
      call<SearchMatch[]>('fs_search', {
        query,
        regex,
        caseSensitive,
        maxResults: maxResults ?? null,
      }),
  },

  history: {
    list: () => lace<RunSummary[]>('history.list'),
    show: (id?: string | null) => lace<ReportView>('history.show', { id: id ?? null }),
    compare: (id?: string | null, against?: string | null) =>
      lace<RunComparison>('history.compare', { id: id ?? null, against: against ?? null }),
    planCleanup: (cleanup: CleanupRequest) =>
      lace<string[]>('history.plan_cleanup', { cleanup: cleanupOf(cleanup) }),
    clean: (ids: string[]) => lace<CleanReport>('history.clean', { ids }),
  },
  /** Os exercícios do `aurora learn` (commands/learn.rs); a correção é o fluxo `learn`. */
  learn: {
    tracks: () => lace<LearnTracks>('learn.tracks'),
    /** Abre a pasta de exercícios que contém `root` e a põe em dia com a
     * trilha (`learn.status`). */
    open: (root: string, lang?: string | null) => lace<LearnSnapshot>('learn.status', { root, lang: lang ?? null }),
    init: (dir: string, track?: string | null, lang?: string | null) =>
      lace<LearnSnapshot>('learn.init', { dir, track: track ?? null, lang: lang ?? null }),
    setCurrent: (root: string, name: string, lang?: string | null) =>
      lace<LearnSnapshot>('learn.set_current', { root, name, lang: lang ?? null }),
    reset: (root: string, name: string) => lace<void>('learn.reset', { root, name }),
  },

  /** Os recursos experimentais do Lace, pelo servidor: o mesmo
   * `experimental.json` do `aurora experimental` da CLI. */
  experimental: {
    list: () => lace<ExperimentalReport>('experimental.list'),
    set: (feature: Feature, enabled: boolean) =>
      lace<ExperimentalReport>('experimental.set', { feature, enabled }),
  },

  /** A placa FPGA, experimental: desligada, cada método daqui responde
   * `experimental_disabled`. Tudo passa pelo servidor; a compilação é o
   * fluxo `fpga_build`. */
  fpga: {
    boards: () => lace<Board[]>('fpga.boards'),
    /** O `fpga.json`; `null` se ainda não existe. */
    config: () => lace<FpgaConfig | null>('fpga.config'),
    setConfig: (config: FpgaConfig) => lace<void>('fpga.config_set', { config }),
    /** O topo que vai para a placa e as portas dele. */
    top: (top?: string | null) => lace<ModuleInterface>('fpga.top', { top: top ?? null }),
    check: () => lace<FpgaPrepared>('fpga.check'),
    /** Os módulos do projeto que podem ir para a placa. */
    modules: () => lace<string[]>('fpga.modules'),
    /** O arquivo de gravação: pronto, desatualizado (e por quê) ou ausente. */
    status: () => lace<BitstreamState>('fpga.status'),
    /** Os cabos de gravação ligados (`quartus_pgm -l`, cerca de 1 s). */
    cables: () => lace<string[]>('fpga.cables'),
  },

  toolchain: {
    info: () => call<ToolchainInfo>('toolchain_info'),
    verify: () => lace<FileMismatch[]>('toolchain.verify'),
    updateCheck: () => call<unknown>('lace_update_check'),
    install: (components: string[], onMessage: (m: JobMessage) => void) => {
      const channel = new Channel<JobMessage>();
      channel.onmessage = onMessage;
      return call<number>('lace_install', { components, channel });
    },
    /** `aurora update --yes --json`: só depois de o usuário confirmar. */
    update: (onMessage: (m: JobMessage) => void) => {
      const channel = new Channel<JobMessage>();
      channel.onmessage = onMessage;
      return call<number>('lace_update', { channel });
    },
  },

  flow: {
    /** Começa um fluxo; as mensagens chegam em `onMessage`, terminando em
     * `finished` ou `failed`. Devolve o identificador da operação. */
    start: (request: FlowRequest, onMessage: (m: JobMessage) => void) => {
      const channel = new Channel<JobMessage>();
      channel.onmessage = onMessage;
      return call<number>('flow_start', { request, channel });
    },
    cancel: (job?: number | null) => call<boolean>('flow_cancel', { job: job ?? null }),
    running: () => call<number | null>('flow_running'),
  },

  assistant: {
    info: () => call<AssistantInfo>('assistant_info'),
    /** Abre uma sessão; os eventos chegam por `onEvent`, começando por `ready`. */
    start: (resume: string | null, onEvent: (event: AgentEvent) => void) => {
      const channel = new Channel<AgentEvent>();
      channel.onmessage = onEvent;
      return call<number>('assistant_start', { resume, channel });
    },
    send: (id: number, text: string) => call<void>('assistant_send', { id, text }),
    interrupt: (id: number) => call<void>('assistant_interrupt', { id }),
    stop: (id: number) => call<void>('assistant_stop', { id }),
  },

  terminal: {
    spawn: (
      cwd: string | null,
      cols: number,
      rows: number,
      onMessage: (m: TerminalMessage) => void,
    ) => {
      const channel = new Channel<TerminalMessage>();
      channel.onmessage = onMessage;
      return call<number>('terminal_spawn', { cwd, cols, rows, channel });
    },
    write: (id: number, data: string) => call<void>('terminal_write', { id, data }),
    /** O xterm escreveu `units` (o `length` das strings) do que o terminal
     * mandou: sem as confirmações, o backend para de ler o shell. */
    ack: (id: number, units: number) => call<void>('terminal_ack', { id, units }),
    /** Digita no shell o `cd` para `path`, na sintaxe do shell dele. */
    cd: (id: number, path: string) => call<void>('terminal_cd', { id, path }),
    resize: (id: number, cols: number, rows: number) =>
      call<void>('terminal_resize', { id, cols, rows }),
    kill: (id: number) => call<void>('terminal_kill', { id }),
  },

  events: {
    onFsChanged: (handler: (event: FsChanged) => void): Promise<UnlistenFn> =>
      listen<FsChanged>('studio://fs-changed', (e) => handler(e.payload)),
  },
};
