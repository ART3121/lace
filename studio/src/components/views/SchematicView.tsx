// O RTLViewer numa aba do Studio: o viewer do pacote @aurora/rtl-viewer, com o
// que só o Studio sabe fazer. O netlist acompanha a síntese do Studio (ou a
// última que ficou no disco), o "Ir ao código" abre a aba do editor, a
// exportação passa pelo diálogo de salvar, e "Sintetizar de novo" é o
// runSynthesis. Os botões e o menu de contexto são os do Studio.

import { save } from '@tauri-apps/plugin-dialog';
import workerUrl from 'elkjs/lib/elk-worker.min.js?url';
import { RtlViewer, setLayoutWorker, type SourceRef } from '@aurora/rtl-viewer';
import { useCallback, useEffect, useState } from 'react';

import { runSynthesis } from '../../actions';
import { useLang, useT } from '../../i18n';
import { api } from '../../ipc/api';
import { useEditor } from '../../state/editor';
import { useJobs } from '../../state/jobs';
import { useProject } from '../../state/project';
import { netlistReader, useSchematic } from '../../state/schematic';
import { showError, useToasts } from '../../state/toasts';
import { joinPath, separator } from '../../util/paths';
import { Button, Empty, IconButton, openContextMenu, Spinner } from '../common';

// O worker do ELK, servido pelo Vite como arquivo.
setLayoutWorker(() => new Worker(workerUrl));

const STUDIO_UI = { Button, IconButton, Spinner, openContextMenu };

/** O netlist da última síntese deste alvo que ficou no disco, para depois
 * de reabrir o Studio. */
function useNetlistOnDisk(enabled: boolean): string | null {
  const snapshot = useProject((s) => s.snapshot);
  const processor = useProject((s) => s.target);
  const [found, setFound] = useState<string | null>(null);
  // A pasta da síntese tem o nome do alvo: o processador ou o topo.
  const target = processor ?? snapshot?.top_module ?? null;
  useEffect(() => {
    setFound(null);
    if (!enabled || !snapshot || !target) return;
    const path = joinPath(snapshot.root, '.aurora8', 'Temp', 'synth', target, 'hierarchy.json');
    let cancelled = false;
    void api.fs
      .stat(path)
      .then((stat) => !cancelled && stat && !stat.is_dir && setFound(path))
      .catch(() => undefined);
    return () => {
      cancelled = true;
    };
  }, [enabled, snapshot, target]);
  return found;
}

export function SchematicView() {
  const t = useT();
  const lang = useLang((s) => s.lang);
  const synthesis = useJobs((s) => s.synthesis);
  const running = useJobs((s) => s.running);
  const source = useSchematic((s) => s.source);
  const loading = useSchematic((s) => s.loading);
  const loadError = useSchematic((s) => s.error);
  const load = useSchematic((s) => s.load);

  // O netlist acompanha a síntese: cada síntese nova regrava o arquivo.
  useEffect(() => {
    if (synthesis?.netlist) void load(synthesis.netlist, netlistReader);
  }, [synthesis?.netlist, synthesis?.duration_ms, load]);
  const onDisk = useNetlistOnDisk(!synthesis?.netlist && !source);

  const openSource = useCallback(
    (src: SourceRef) => {
      // O Yosys grava o caminho com '/'; as abas do Studio usam o separador
      // do sistema, o mesmo do netlist.
      const native = source && separator(source.path) === '\\' ? src.file.replace(/\//g, '\\') : src.file;
      void useEditor.getState().openFile(native, { line: src.line, column: src.column });
    },
    [source],
  );

  const exportFile = useCallback(
    async (fileName: string, svg: string) => {
      const root = useProject.getState().snapshot?.root;
      const file = await save({
        defaultPath: root ? joinPath(root, fileName) : fileName,
        filters: [{ name: 'SVG', extensions: ['svg'] }],
      });
      if (!file) return;
      try {
        await api.fs.writeText(file, svg);
        useToasts.getState().push({ kind: 'success', title: t('schematic.exported'), detail: file });
      } catch (error) {
        showError(error);
      }
    },
    [t],
  );

  const empty = (
    <div className="view-page">
      <Empty>
        {loading ? (
          <p>
            <Spinner /> {t('schematic.loading')}
          </p>
        ) : (
          <>
            <p>{loadError ? t('schematic.loadFailed', { message: loadError }) : t('schematic.empty')}</p>
            <div className="rtlViewer-empty__actions">
              <Button variant="primary" disabled={!!running} onClick={() => void runSynthesis()}>
                {t('action.synthesize')}
              </Button>
              {onDisk && <Button onClick={() => void load(onDisk, netlistReader)}>{t('schematic.lastSynthesis')}</Button>}
            </div>
          </>
        )}
      </Empty>
    </div>
  );

  return (
    <RtlViewer
      store={useSchematic}
      lang={lang}
      empty={empty}
      ui={STUDIO_UI}
      onOpenSource={openSource}
      onExport={exportFile}
      onResynthesize={() => void runSynthesis()}
      resynthesizing={running?.flow === 'synthesize'}
      resynthesizeDisabled={!!running}
    />
  );
}
