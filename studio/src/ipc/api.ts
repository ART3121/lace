// As chamadas ao backend, uma função por comando Tauri, com os tipos de
// entrada e saída. O resto da interface só fala com o backend por aqui.
// Referência: docs/IPC.md.

import { Channel, invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';

import type {
  AddedFile,
  FileMismatch,
  ProjectFile,
  RunComparison,
  RunRecord,
  RunSummary,
} from './lace-types';
import type {
  AppInfo,
  CleanReport,
  CleanupRequest,
  DirEntry,
  FileStat,
  FlowRequest,
  FsChanged,
  HierarchyResult,
  IpcError,
  JobMessage,
  ListPosition,
  MovedPath,
  NewProcessorDefaults,
  NewProcessorRequest,
  Processor,
  ProjectSnapshot,
  RecentStatus,
  SearchMatch,
  Settings,
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
    openWave: (processor?: string | null, path?: string | null) =>
      call<WaveOpened>('wave_open', { processor: processor ?? null, path: path ?? null }),
  },

  /** A onda numa aba (`wave_tab.rs`). */
  waveTab: {
    open: (path: string) => call<WaveTab>('wave_tab_open', { path }),
    close: (id: string) => call<void>('wave_tab_close', { id }),
  },

  project: {
    open: (path: string) => call<ProjectSnapshot>('project_open', { path }),
    create: (parent: string, name: string) =>
      call<ProjectSnapshot>('project_create', { parent, name }),
    close: () => call<void>('project_close'),
    snapshot: () => call<ProjectSnapshot>('project_snapshot'),
    addVerilog: (path: string, testbench: boolean) =>
      call<AddedFile>('project_add_verilog', { path, testbench }),
    removeVerilog: (path: string) => call<boolean>('project_remove_verilog', { path }),
    setTop: (target: string) => call<string>('project_set_top', { target }),
    setTestbench: (path: string) => call<void>('project_set_testbench', { path }),
    processorDefaults: () => call<NewProcessorDefaults>('project_processor_defaults'),
    addProcessor: (request: NewProcessorRequest) =>
      call<Processor>('project_add_processor', { request }),
    configureProcessor: (
      name: string,
      config: { frequency_mhz?: number | null; clocks?: number | null; show_arrays?: boolean | null },
    ) =>
      call<Processor>('project_configure_processor', {
        name,
        frequencyMhz: config.frequency_mhz ?? null,
        clocks: config.clocks ?? null,
        showArrays: config.show_arrays ?? null,
      }),
    outputValues: (processor: string, port: number) =>
      call<number[]>('project_output_values', { processor, port }),
    verilogModules: (text: string) => call<string[]>('verilog_modules', { text }),
    /** Move ou renomeia (`Project::move_path` do Core): `to` é o caminho
     * final. Erro `path_exists` se ele já existe, sem `overwrite`. */
    move: (from: string, to: string, overwrite = false) =>
      call<MovedPath>('project_move', { from, to, overwrite }),
    /** `null` se o nome de projeto serve; senão o erro do Core. */
    checkName: (name: string) => call<IpcError | null>('project_check_name', { name }),
    /** Muda a posição de um arquivo na lista dele (`lace order`); devolve
     * a lista na ordem nova. */
    reorder: (path: string, position: ListPosition) =>
      call<ProjectFile[]>('project_reorder', { path, position }),
    /** `null` se o nome de processador serve; senão o erro do Core. */
    checkProcessorName: (name: string) => call<IpcError | null>('processor_check_name', { name }),
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
    list: () => call<RunSummary[]>('history_list'),
    show: (id?: string | null) => call<ReportView>('history_show', { id: id ?? null }),
    compare: (id?: string | null, against?: string | null) =>
      call<RunComparison>('history_compare', { id: id ?? null, against: against ?? null }),
    planCleanup: (cleanup: CleanupRequest) => call<string[]>('history_plan_cleanup', { cleanup }),
    clean: (ids: string[]) => call<CleanReport>('history_clean', { ids }),
  },

  toolchain: {
    info: () => call<ToolchainInfo>('toolchain_info'),
    verify: () => call<FileMismatch[]>('toolchain_verify'),
    updateCheck: () => call<unknown>('lace_update_check'),
    install: (components: string[], onMessage: (m: JobMessage) => void) => {
      const channel = new Channel<JobMessage>();
      channel.onmessage = onMessage;
      return call<number>('lace_install', { components, channel });
    },
    /** `lace update --yes --json`: só depois de o usuário confirmar. */
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
