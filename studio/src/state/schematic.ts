// O estado do RTLViewer no Studio: o do pacote @lace/rtl-viewer, um só para
// a vida do Studio (a vista desmonta quando outra aba vem para a frente, e
// voltar a ela volta ao mesmo lugar), e o leitor do netlist pelo backend.
//
// O netlist é o `hierarchy.json` da última síntese (SynthesisResult.netlist)
// ou, depois de reabrir o Studio, o que ficou em `.aurora8/Temp/synth/<alvo>/`.

import { createRtlViewerStore, type NetlistReader } from '@lace/rtl-viewer';

import { api } from '../ipc/api';

export const useSchematic = createRtlViewerStore({ storageKey: 'aurora-ide:rtlViewer' });

/** Lê o netlist pelo backend do Studio. */
export const netlistReader: NetlistReader = {
  modified: async (path) => (await api.fs.stat(path))?.modified_ms ?? null,
  read: async (path) => {
    const file = await api.fs.readText(path);
    if (file.too_large) throw new Error(`${path}: too large`);
    return { text: file.content, modifiedMs: file.modified_ms };
  },
};
