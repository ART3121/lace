// A área de trabalho: a barra de atividades (à esquerda ou à direita), as
// duas barras laterais e, entre elas, o editor com o painel embaixo ou à
// direita, com as divisões redimensionáveis de `react-resizable-panels`.
//
// Três cuidados com a biblioteca:
//
// - Os lugares na árvore não mudam e nada tem `key` tirada do layout: a
//   barra lateral esquerda, o centro e a direita são sempre o primeiro, o
//   segundo e o terceiro filho do grupo, e trocar a posição do painel só muda
//   a orientação do grupo do centro. Assim o editor nunca é remontado (o
//   Monaco perderia o desfazer, e uma aba de onda recarregaria).
// - O grupo lembra um tamanho para cada conjunto de painéis, e essa memória
//   vence o `defaultSize`. Por isso os tamanhos do layout entram com
//   `resize`, um quadro depois de um layout aplicado ou de uma região aparecer.
// - Os tamanhos só são gravados quando o usuário arrasta uma divisão
//   (`isUserInteraction`), nunca ao montar, ao redimensionar a janela ou
//   quando a biblioteca acomoda os mínimos. As laterais e o painel mantêm os
//   pixels quando a janela muda de tamanho (`preserve-pixel-size`).
//
// Numa janela estreita, primeiro os mínimos diminuem; se ainda não couber, a
// barra lateral direita deixa de ser desenhada e depois o painel volta para
// baixo. Isso só muda o desenho: o layout continua o mesmo.

import { useEffect, useRef, useState } from 'react';
import { Group, Panel, Separator, usePanelRef, type PanelImperativeHandle } from 'react-resizable-panels';

import { useLayout } from '../../state/layout';
import type { LayoutSizes } from '../../state/layoutModel';
import { snapshotSize } from '../../state/savedLayouts';
import { EditorArea } from '../editor/EditorArea';
import { ActivityBar } from './ActivityBar';
import { Region } from './Region';
import { ZenShell } from './Zen';

/** Os mínimos de cada folga, do confortável ao apertado (pixels CSS). */
const TIERS = [
  { side: 190, centerBottom: 320, editorRight: 240, panelRight: 180 },
  { side: 140, centerBottom: 200, editorRight: 140, panelRight: 140 },
] as const;

interface Fit {
  side: number;
  center: number;
  editorRight: number;
  panelRight: number;
  showRight: boolean;
  panelAtRight: boolean;
}

/** O que cabe em `width` pixels, com as regiões pedidas. */
function fitWorkbench(width: number, left: boolean, right: boolean, panelAtRight: boolean): Fit {
  const need = (side: number, center: number, withRight: boolean) => (left ? side + 1 : 0) + (withRight ? side + 1 : 0) + center;
  const result = (tier: (typeof TIERS)[number], showRight: boolean, atRight: boolean): Fit => ({
    side: tier.side,
    center: atRight ? tier.editorRight + tier.panelRight + 1 : tier.centerBottom,
    editorRight: tier.editorRight,
    panelRight: tier.panelRight,
    showRight,
    panelAtRight: atRight,
  });
  // Antes da primeira medida, nada é cortado.
  if (width <= 0) return result(TIERS[0], right, panelAtRight);
  for (const tier of TIERS) {
    const center = panelAtRight ? tier.editorRight + tier.panelRight + 1 : tier.centerBottom;
    if (need(tier.side, center, right) <= width) return result(tier, right, panelAtRight);
  }
  const tight = TIERS[1];
  const centerAtRight = tight.editorRight + tight.panelRight + 1;
  if (need(tight.side, panelAtRight ? centerAtRight : tight.centerBottom, false) <= width) return result(tight, false, panelAtRight);
  return result(tight, false, false);
}

/** O tamanho de um painel montado, em pixels; `undefined` se ainda não há. */
function sizeOf(ref: React.RefObject<PanelImperativeHandle | null>): number | undefined {
  try {
    const size = ref.current?.getSize().inPixels;
    return size && size > 0 ? size : undefined;
  } catch {
    return undefined;
  }
}

export function Workbench() {
  const live = useLayout((s) => s.live);
  const zen = useLayout((s) => s.zen);
  const zenShell = useLayout((s) => s.zenShell);
  const maximized = useLayout((s) => s.panelMaximized);
  const epoch = useLayout((s) => s.epoch);
  const { regions, bars, sizes } = live;

  const [width, setWidth] = useState(0);
  const groupElement = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const element = groupElement.current;
    if (!element) return;
    const observer = new ResizeObserver(([entry]) => setWidth(Math.round(entry.contentRect.width)));
    observer.observe(element);
    return () => observer.disconnect();
  }, []);

  const activity = zen ? 'hidden' : bars.activitybar;
  // No zen, a gaveta do shell fica embaixo do editor: o grupo do centro é
  // vertical, mesmo com o painel à direita no layout.
  const fit = fitWorkbench(width, regions.left.visible, regions.right.visible, !zen && live.panelPosition === 'right');
  const show = { left: regions.left.visible, right: fit.showRight, panel: regions.panel.visible };
  const bottom = !fit.panelAtRight;

  const leftRef = usePanelRef();
  const rightRef = usePanelRef();
  const panelRef = usePanelRef();

  // O que a captura precisa saber sem depender de uma renderização velha.
  const current = useRef({ bottom, maximized });
  current.current = { bottom, maximized };

  // Põe os tamanhos do layout nos painéis montados, no próximo quadro (um
  // pedido por quadro). Um tamanho que os mínimos não deixam caber fica onde
  // a biblioteca o põe; ela não avisa mudança quando nada muda, então isso
  // não volta a se chamar sozinho.
  const frame = useRef(0);
  const enforce = useRef(() => {});
  enforce.current = () => {
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      const state = useLayout.getState();
      const want = (ref: React.RefObject<PanelImperativeHandle | null>, size: number) => {
        const now = sizeOf(ref);
        if (now !== undefined && Math.abs(now - size) > 1) ref.current?.resize(size);
      };
      want(leftRef, state.live.sizes.left);
      want(rightRef, state.live.sizes.right);
      if (!state.panelMaximized) want(panelRef, current.current.bottom ? state.live.sizes.panelBottom : state.live.sizes.panelRight);
    });
  };
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  // Arrastar uma divisão grava os tamanhos; qualquer outra mudança (um painel
  // que apareceu, a orientação trocada, a janela redimensionada, a memória da
  // biblioteca) volta aos tamanhos do layout.
  const capture = (_layout: unknown, meta: { isUserInteraction: boolean }) => {
    if (!meta.isUserInteraction) {
      enforce.current();
      return;
    }
    const { bottom: atBottom, maximized: full } = current.current;
    const panel = full ? undefined : sizeOf(panelRef);
    useLayout.getState().setSizes({
      left: sizeOf(leftRef),
      right: sizeOf(rightRef),
      ...(atBottom ? { panelBottom: panel } : { panelRight: panel }),
    });
  };

  // E também depois de um layout aplicado ou de uma região aparecer: a
  // biblioteca registra um painel novo numa renderização a mais, e só então
  // ele aceita `resize`.
  useEffect(() => enforce.current(), [epoch, show.left, show.right, show.panel, bottom, maximized, width]);

  // O duplo clique numa divisão volta a região ao tamanho da foto do layout
  // em uso. O da biblioteca voltaria ao `defaultSize`, que aqui já é o
  // tamanho de agora.
  const resetSize = (size: keyof LayoutSizes) => {
    useLayout.getState().setSizes({ [size]: snapshotSize(size) });
    enforce.current();
  };

  return (
    <div className="workbench">
      {activity === 'left' && <ActivityBar side="left" />}
      <Group orientation="horizontal" className="workbench__main" elementRef={groupElement} onLayoutChanged={capture}>
        {show.left && (
          <>
            <Panel
              id="left"
              panelRef={leftRef}
              defaultSize={sizes.left}
              minSize={fit.side}
              maxSize="40%"
              groupResizeBehavior="preserve-pixel-size"
            >
              <Region region="left" mode={activity === 'left' ? 'title' : 'tabs'} />
            </Panel>
            <Separator className="resize-handle resize-handle--vertical" disableDoubleClick onDoubleClick={() => resetSize('left')} />
          </>
        )}
        <Panel id="center" minSize={fit.center}>
          <Group orientation={bottom ? 'vertical' : 'horizontal'} className="workbench__center" onLayoutChanged={capture}>
            {!(show.panel && maximized) && (
              <Panel id="editor" minSize={bottom ? 120 : fit.editorRight}>
                <EditorArea />
              </Panel>
            )}
            {zen && zenShell && (
              <>
                <Separator className="resize-handle resize-handle--horizontal resize-handle--zen" />
                <Panel id="zen-shell" defaultSize="35%" minSize={100}>
                  <ZenShell />
                </Panel>
              </>
            )}
            {show.panel && (
              <>
                {!maximized && (
                  <Separator
                    className={`resize-handle ${bottom ? 'resize-handle--horizontal' : 'resize-handle--vertical'}`}
                    disableDoubleClick
                    onDoubleClick={() => resetSize(bottom ? 'panelBottom' : 'panelRight')}
                  />
                )}
                <Panel
                  // Um id por orientação: a memória de tamanhos da biblioteca
                  // é por conjunto de ids, e a proporção de quando o painel
                  // estava embaixo não serve para ele à direita.
                  id={bottom ? 'panel' : 'panel-right'}
                  panelRef={panelRef}
                  defaultSize={bottom ? sizes.panelBottom : sizes.panelRight}
                  minSize={bottom ? 90 : fit.panelRight}
                  // Maximizado, o painel é o único do grupo, e a biblioteca
                  // exige um painel que acompanhe o tamanho do grupo.
                  groupResizeBehavior={maximized ? 'preserve-relative-size' : 'preserve-pixel-size'}
                >
                  <Region region="panel" mode="tabs" />
                </Panel>
              </>
            )}
          </Group>
        </Panel>
        {show.right && (
          <>
            <Separator className="resize-handle resize-handle--vertical" disableDoubleClick onDoubleClick={() => resetSize('right')} />
            <Panel
              id="right"
              panelRef={rightRef}
              defaultSize={sizes.right}
              minSize={fit.side}
              maxSize="40%"
              groupResizeBehavior="preserve-pixel-size"
            >
              <Region region="right" mode={activity === 'right' ? 'title' : 'tabs'} />
            </Panel>
          </>
        )}
      </Group>
      {activity === 'right' && <ActivityBar side="right" />}
    </div>
  );
}
