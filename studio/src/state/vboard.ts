// A placa virtual: a sessão do `vboard.run` (ipc/api.ts, `vboard`) e o que
// a vista desenha. O estado fica aqui, e não na vista: trocar de aba
// desmonta a vista, e a placa continua ligada.

import { create } from 'zustand';

import { api } from '../ipc/api';
import type { BoardInput, BoardLayout, Diagnostic, Reconnect } from '../ipc/aurora-types';
import type { IpcError, ModuleInterface, VboardMessage } from '../ipc/types';
import { t } from '../i18n';
import { useEditor } from './editor';
import { useProject } from './project';
import { showError } from './toasts';

export type VboardStatus = 'idle' | 'building' | 'running' | 'stopping';

/** O máximo de texto guardado por UART e de linhas do `$display`. */
const UART_LIMIT = 16_000;
const OUTPUT_LIMIT = 300;

interface VboardFrame {
  device: string;
  width: number;
  height: number;
  /** `data:image/png;base64,...`, direto no `src` de uma imagem. */
  url: string;
}

interface VboardState {
  status: VboardStatus;
  /** O id do pedido `vboard.run`, que as entradas citam. */
  run: number | null;
  /** A placa, de `vboard.layout` antes de ligar e do `vboard_ready` depois. */
  layout: BoardLayout | null;
  /** Por que não há placa (sem `fpga.json`, ligação inválida). */
  layoutError: IpcError | null;
  /** A placa escolhida num projeto sem `fpga.json` (as portas pelo nome). */
  board: string | null;
  /** A compilação falhou: os erros do Verilator e do compilador. */
  diagnostics: Diagnostic[];
  /** A última linha da compilação, para mostrar o andamento. */
  buildLine: string | null;
  /** O fim da última sessão, quando não foi o usuário que parou. */
  ended: string | null;
  lights: Record<string, number[]>;
  frames: Record<string, VboardFrame>;
  uart: Record<string, string>;
  /** A velocidade de cada serial que o projeto não transmite na da placa:
   * a placa detectou outra e passou a ler nela. */
  uartBaud: Record<string, number>;
  output: string[];
  timePs: number;
  speed: number;
  paused: boolean;
  /** Tempo real (1) ou o mais rápido possível (0). */
  realtime: boolean;
  /** O valor das chaves e dos botões apertados, por periférico. */
  inputs: Record<string, number>;
  /** O modo de editar as ligações: um clique num elemento da placa escolhe
   * a porta do topo que vai nele. */
  editing: boolean;
  /** As portas do topo, para escolher na edição. */
  ports: ModuleInterface['ports'] | null;
  loadLayout: () => Promise<void>;
  setEditing: (editing: boolean) => void;
  /** Liga um elemento da placa a uma porta (ou o solta), grava o
   * `fpga.json` e lê a placa de novo. */
  connect: (change: Reconnect) => Promise<void>;
  chooseBoard: (board: string | null) => Promise<void>;
  start: () => Promise<void>;
  stop: () => void;
  send: (input: BoardInput) => void;
  setBit: (device: string, bit: number, on: boolean) => void;
  setRealtime: (realtime: boolean) => void;
  togglePause: () => void;
  clearUart: (device: string) => void;
}

const initialSession = {
  run: null,
  diagnostics: [],
  buildLine: null,
  lights: {},
  frames: {},
  uart: {},
  uartBaud: {},
  output: [],
  timePs: 0,
  speed: 0,
  paused: false,
};

/** Um decodificador de UTF-8 por serial, para o caractere de vários bytes
 * que chega partido entre dois estados. */
const decoders = new Map<string, TextDecoder>();
function decoderOf(device: string): TextDecoder {
  let decoder = decoders.get(device);
  if (!decoder) {
    decoder = new TextDecoder('utf-8');
    decoders.set(device, decoder);
  }
  return decoder;
}

export const useVboard = create<VboardState>()((set, get) => {
  function onMessage(message: VboardMessage) {
    switch (message.type) {
      case 'started':
        set({ run: message.run });
        return;
      case 'events': {
        // A compilação: a última linha de saída dos passos, como andamento.
        for (const event of message.events) {
          if (event.event === 'output' && event.line.trim()) set({ buildLine: event.line });
        }
        return;
      }
      case 'vboard_ready': {
        set({ status: 'running', layout: message.layout, buildLine: null });
        // As chaves e os botões voltam ao que a vista mostra.
        const { inputs, run } = get();
        if (run !== null) {
          for (const [device, value] of Object.entries(inputs)) {
            void api.vboard.input(run, { kind: 'set', device, value }).catch(() => undefined);
          }
        }
        return;
      }
      case 'vboard_state': {
        const state = message.state;
        const uart = { ...get().uart };
        let changed = false;
        for (const [device, bytes] of Object.entries(state.uart)) {
          if (!bytes?.length) continue;
          // UTF-8, com o caractere que veio partido entre dois estados
          // guardado para o próximo; os bytes que não formam UTF-8 viram �.
          const text = (uart[device] ?? '') + decoderOf(device).decode(new Uint8Array(bytes), { stream: true });
          uart[device] = text.length > UART_LIMIT ? text.slice(-UART_LIMIT) : text;
          changed = true;
        }
        const lights: Record<string, number[]> = {};
        for (const [signal, levels] of Object.entries(state.lights)) if (levels) lights[signal] = levels;
        set({
          lights,
          timePs: state.time_ps,
          speed: state.speed,
          paused: state.paused,
          uartBaud: Object.fromEntries(
            Object.entries(state.uart_baud ?? {}).filter((entry): entry is [string, number] => entry[1] !== undefined),
          ),
          ...(changed ? { uart } : {}),
        });
        return;
      }
      case 'vboard_frame': {
        const frame = message.frame;
        set({
          frames: {
            ...get().frames,
            [frame.device]: {
              device: frame.device,
              width: frame.width,
              height: frame.height,
              url: `data:image/png;base64,${frame.png}`,
            },
          },
        });
        return;
      }
      case 'vboard_output': {
        // Um lote por estado, de até 100 linhas; o resto o servidor só conta.
        const output = [...get().output, ...message.lines];
        if (message.omitted > 0) output.push(t('vboard.outputOmitted', { count: message.omitted }));
        set({ output: output.length > OUTPUT_LIMIT ? output.slice(-OUTPUT_LIMIT) : output });
        return;
      }
      case 'finished': {
        const { build, outcome } = message.result;
        const ended =
          outcome?.reason === 'crashed'
            ? 'crashed'
            : outcome?.reason === 'finished'
              ? 'finished'
              : build.status !== 'succeeded'
                ? 'build_failed'
                : null;
        set({
          status: 'idle',
          run: null,
          // Sem placa quando um processador SAPHO não compilou: fica a de antes.
          layout: build.layout ?? get().layout,
          diagnostics: build.status === 'succeeded' ? [] : build.diagnostics,
          ended,
          buildLine: null,
        });
        return;
      }
      case 'failed':
        set({ status: 'idle', run: null, buildLine: null });
        showError(message.error);
        return;
      default:
        return;
    }
  }

  return {
    status: 'idle',
    layout: null,
    layoutError: null,
    board: null,
    ended: null,
    realtime: true,
    inputs: {},
    editing: false,
    ports: null,
    ...initialSession,

    loadLayout: async () => {
      try {
        const layout = await api.vboard.layout(get().board);
        set({ layout, layoutError: null });
      } catch (error) {
        set({ layout: null, layoutError: error as IpcError });
      }
      try {
        const top = await api.fpga.top(get().layout?.top ?? null);
        set({ ports: top.ports });
      } catch {
        set({ ports: null });
      }
    },

    setEditing: (editing) => set({ editing }),

    connect: async (change) => {
      try {
        await api.fpga.connect(change, get().board);
        // Com o fpga.json gravado, a placa vem dele.
        set({ board: null });
        await get().loadLayout();
      } catch (error) {
        showError(error);
      }
    },

    chooseBoard: async (board) => {
      set({ board, inputs: {} });
      await get().loadLayout();
    },

    start: async () => {
      if (get().status !== 'idle') return;
      // Os arquivos abertos são gravados antes, como nas operações do fluxo
      // (jobs.ts): a placa compila o que está na tela, e não o que estava no
      // disco. Quem cancela a gravação não liga a placa.
      if (!(await useEditor.getState().saveAll())) return;
      if (get().status !== 'idle') return;
      set({ status: 'building', ended: null, ...initialSession });
      decoders.clear();
      try {
        await api.vboard.start({ board: get().board, limit: get().realtime ? 1 : 0, fps: 30 }, onMessage);
      } catch (error) {
        set({ status: 'idle' });
        showError(error);
      }
    },

    stop: () => {
      const { run, status } = get();
      if (run === null || status === 'idle') return;
      set({ status: 'stopping' });
      void api.vboard.stop(run).catch(showError);
    },

    send: (input) => {
      const run = get().run;
      if (run === null || get().status !== 'running') return;
      void api.vboard.input(run, input).catch(showError);
    },

    setBit: (device, bit, on) => {
      const value = get().inputs[device] ?? 0;
      const next = on ? value | (1 << bit) : value & ~(1 << bit);
      set({ inputs: { ...get().inputs, [device]: next } });
      get().send({ kind: 'set', device, value: next >>> 0 });
    },

    setRealtime: (realtime) => {
      set({ realtime });
      get().send({ kind: 'speed', limit: realtime ? 1 : 0 });
    },

    togglePause: () => {
      get().send(get().paused ? { kind: 'resume' } : { kind: 'pause' });
    },

    clearUart: (device) => {
      const uart = { ...get().uart };
      delete uart[device];
      decoders.delete(device);
      set({ uart });
    },
  };
});

// Outro projeto: a placa do anterior desliga, e a do novo é lida de novo.
let lastSpf: string | null = null;
useProject.subscribe((state) => {
  const spf = state.snapshot?.spf ?? null;
  if (spf === lastSpf) return;
  lastSpf = spf;
  useVboard.getState().stop();
  useVboard.setState({
    layout: null,
    layoutError: null,
    board: null,
    inputs: {},
    ended: null,
    editing: false,
    ports: null,
    ...initialSession,
  });
});
