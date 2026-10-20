// Os recursos experimentais do Lace: partes que ainda mudam muito e que
// ficam desligadas até o usuário ligar, em Preferências > Experimental.
// Quem guarda o que está ligado é o Core (experimental.json, na pasta de
// configuração do Lace), e a CLI lê o mesmo arquivo: ligar aqui também
// libera o `lace fpga`, e um `lace experimental enable` dado num terminal
// vale aqui quando a janela volta a ter o foco (App.tsx).
//
// Desligado, um recurso sai da janela: não fica cinza, some. Hoje há dois.
// A placa FPGA (`fpga`): a aba Placa FPGA, o botão Placa da barra de
// ferramentas, as ações dela (menu Fluxo, paleta, navegador de fluxo), o
// console Placa e o Quartus nas Preferências e na tela do bundle. O
// assistente de IA (`assistant`): a vista Assistente, as ações dela (menu
// Exibir, paleta) e a página Assistente das Preferências. Cada lugar sabe o
// recurso do que mostra: as ações em actions.ts, as vistas em
// layoutModel.ts, as abas em editor.ts, as páginas em SettingsView.tsx. O
// backend recusa os comandos e os fluxos dele com `experimental_disabled`.

import { create } from 'zustand';

import { api } from '../ipc/api';
import type { ExperimentalReport, Feature, FeatureState } from '../ipc/lace-types';
import { onErrorCode, showError } from './toasts';

interface ExperimentalState {
  /** Cada recurso e se está ligado; vazio até a primeira leitura. */
  components: FeatureState[];
  /** O `experimental.json`, que as Preferências mostram. */
  path: string | null;
  /** Os ligados. Até a primeira leitura, nenhum: nada experimental aparece
   * antes de o Core dizer que está ligado. */
  enabled: ReadonlySet<Feature>;
  /** Relê do disco. */
  load: () => Promise<void>;
  set: (feature: Feature, enabled: boolean) => Promise<void>;
}

const NONE: ReadonlySet<Feature> = new Set();

function apply(report: ExperimentalReport): void {
  const current = useExperimental.getState().enabled;
  const enabled = new Set(report.components.filter((c) => c.enabled).map((c) => c.feature));
  // O mesmo conjunto fica o mesmo objeto: quem assina `enabled` só
  // redesenha quando um recurso liga ou desliga.
  const same = enabled.size === current.size && [...enabled].every((feature) => current.has(feature));
  useExperimental.setState({ components: report.components, path: report.path, enabled: same ? current : enabled });
}

export const useExperimental = create<ExperimentalState>(() => ({
  components: [],
  path: null,
  enabled: NONE,

  load: async () => {
    try {
      apply(await api.experimental.list());
    } catch (error) {
      showError(error);
    }
  },

  set: async (feature, enabled) => {
    try {
      apply(await api.experimental.set(feature, enabled));
    } catch (error) {
      showError(error);
    }
  },
}));

/** O recurso está ligado agora, fora do React (ações, vistas, abas). */
export function featureOn(feature: Feature): boolean {
  return useExperimental.getState().enabled.has(feature);
}

/** O recurso está ligado; o componente redesenha quando ele muda. */
export function useFeature(feature: Feature): boolean {
  return useExperimental((s) => s.enabled.has(feature));
}

// Um comando de um recurso desligado por fora (pela CLI, com o Studio
// aberto) relê o que está ligado, e o recurso sai da janela. O erro aparece
// mesmo assim, com o caminho para ligar de novo.
onErrorCode('experimental_disabled', () => {
  void useExperimental.getState().load();
  return false;
});
