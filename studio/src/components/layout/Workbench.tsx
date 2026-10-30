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
//
// Abrir, fechar ou mover uma região anima: o desenho troca dentro de uma
// transição de vista (`document.startViewTransition`), e o navegador anima o
// tamanho e o lugar de cada região entre a foto de antes e a de depois
// (app.css, `data-transicao`). O estado do layout muda na hora, como sempre;
// só a cópia que a janela desenha (`useShownLayout`) espera a foto de antes.
// Sem a API, ou com o sistema pedindo menos movimento, a troca é na hora.

import { useEffect, useRef, useState } from 'react';
import { flushSync } from 'react-dom';
import { Group, Panel, Separator, usePanelRef, type PanelImperativeHandle } from 'react-resizable-panels';
import { create } from 'zustand';

import { useEditor } from '../../state/editor';
import { useLayout } from '../../state/layout';
import type { LayoutBody, LayoutSizes } from '../../state/layoutModel';
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

/** O que a área de trabalho desenha do layout. */
interface Shown {
  live: LayoutBody;
  zen: boolean;
  zenShell: boolean;
  maximized: boolean;
  epoch: number;
}

function shownOf(state: ReturnType<typeof useLayout.getState>): Shown {
  return { live: state.live, zen: state.zen, zenShell: state.zenShell, maximized: state.panelMaximized, epoch: state.epoch };
}

/** O layout como a janela o desenha: o do `useLayout`, que numa transição só
 * chega aqui na troca de foto. O App lê o zen daqui, para as barras de cima
 * saírem e voltarem na mesma transição das regiões. */
export const useShownLayout = create<Shown>(() => shownOf(useLayout.getState()));

/** O arranjo da janela: o que muda de lugar e vale a animação (uma região
 * que abre ou fecha, o painel que muda de lado ou maximiza, o zen, a barra
 * de atividades que troca de lado). Tamanhos e vistas ativas não entram. */
function arrangement(shown: Shown): string {
  const { regions, panelPosition, bars } = shown.live;
  return [regions.left.visible, regions.right.visible, regions.panel.visible, panelPosition, bars.activitybar, shown.zen, shown.zenShell, shown.maximized].join();
}

/** As regiões desenhadas antes e depois de uma transição: nelas os botões da
 * direita do cabeçalho viram uma foto à parte e ficam presos à borda direita
 * enquanto a região muda de tamanho (app.css). */
function staying(previous: Shown, next: Shown): string {
  const editor = (shown: Shown) => !(shown.maximized && shown.live.regions.panel.visible);
  const names: string[] = [];
  if (previous.live.regions.panel.visible && next.live.regions.panel.visible) names.push('painel');
  if (editor(previous) && editor(next)) names.push('editor');
  return names.join(' ');
}

/** Cada região pelo nome que tem na transição (app.css). */
const MEASURED = [
  ['esquerda', '.region--left'],
  ['direita', '.region--right'],
  ['painel', '.region--panel'],
  ['editor', '.editor-area'],
] as const;

/** O tamanho de cada região desenhada; o editor escondido pelo painel
 * maximizado não entra. */
function measure(root: HTMLElement | null): Map<string, DOMRect> {
  const sizes = new Map<string, DOMRect>();
  for (const [name, selector] of MEASURED) {
    const rect = root?.querySelector(selector)?.getBoundingClientRect();
    if (rect && rect.width > 0 && rect.height > 0) sizes.set(name, rect);
  }
  return sizes;
}

/** As regiões desenhadas antes e depois que só crescem e as que só encolhem.
 * Essas mostram uma foto só, a maior (app.css); a que cresce numa direção e
 * encolhe na outra, ou só muda de lugar, esmaece entre as duas. */
function growth(before: Map<string, DOMRect>, after: Map<string, DOMRect>): { grows: string; shrinks: string } {
  const grows: string[] = [];
  const shrinks: string[] = [];
  for (const [name, old] of before) {
    const now = after.get(name);
    if (!now) continue;
    const width = now.width - old.width;
    const height = now.height - old.height;
    if (Math.abs(width) < 1 && Math.abs(height) < 1) continue;
    if (width > -1 && height > -1) grows.push(name);
    else if (width < 1 && height < 1) shrinks.push(name);
  }
  return { grows: grows.join(' '), shrinks: shrinks.join(' ') };
}

/** As marcas que uma transição põe no <html> para o app.css. */
const MARKS = ['transicao', 'ficam', 'boasVindas', 'cresce', 'encolhe'] as const;

const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');

/** Uma volta do laço de eventos. Durante a transição de vista o navegador não
 * roda quadros (`requestAnimationFrame`), só tarefas: os efeitos do React e o
 * registro de um painel novo na biblioteca correm nelas. */
const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

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
  const { live, zen, zenShell, maximized, epoch } = useShownLayout();
  const { regions, bars, sizes } = live;
  const workbench = useRef<HTMLDivElement>(null);

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
  const settle = useRef(() => {});
  settle.current = () => {
    const state = useLayout.getState();
    const want = (ref: React.RefObject<PanelImperativeHandle | null>, size: number) => {
      const now = sizeOf(ref);
      if (now !== undefined && Math.abs(now - size) > 1) ref.current?.resize(size);
    };
    want(leftRef, state.live.sizes.left);
    want(rightRef, state.live.sizes.right);
    if (!state.panelMaximized) want(panelRef, current.current.bottom ? state.live.sizes.panelBottom : state.live.sizes.panelRight);
  };
  const enforce = useRef(() => {});
  enforce.current = () => {
    if (frame.current) return;
    frame.current = requestAnimationFrame(() => {
      frame.current = 0;
      settle.current();
    });
  };
  useEffect(() => () => cancelAnimationFrame(frame.current), []);

  // O desenho segue o layout: na hora, quando só mudam tamanhos e vistas;
  // numa transição de vista, quando muda o arranjo. Enquanto a transição não
  // desenha o depois, uma mudança nova só troca o alvo dela.
  const target = useRef<Shown | null>(null);
  useEffect(() => {
    const setShown = (shown: Shown) => useShownLayout.setState(shown, true);
    let drawn = shownOf(useLayout.getState());
    setShown(drawn);
    let running = 0;
    const root = document.documentElement;
    return useLayout.subscribe((state) => {
      const next = shownOf(state);
      if (target.current) {
        target.current = next;
        drawn = next;
        return;
      }
      const previous = drawn;
      drawn = next;
      if (arrangement(next) === arrangement(previous) || !('startViewTransition' in document) || reducedMotion.matches) {
        setShown(next);
        return;
      }
      target.current = next;
      // As marcas que dão nome às fotos valem já para a foto de antes; as
      // que escolhem a animação de cada uma, só depois da troca.
      const side = () => (current.current.bottom ? 'baixo' : 'direita');
      const before = measure(workbench.current);
      root.dataset.transicao = side();
      root.dataset.ficam = staying(previous, next);
      root.toggleAttribute('data-boas-vindas', !!workbench.current?.querySelector('.editor-area .welcome'));
      running += 1;
      const transition = document.startViewTransition(async () => {
        const after = target.current ?? next;
        target.current = null;
        flushSync(() => setShown(after));
        // A biblioteca registra um painel novo numa renderização a mais, e
        // só então ele aceita os tamanhos do layout, que fora da transição
        // entram um quadro depois (`enforce`): aqui entram antes da foto.
        await tick();
        await tick();
        flushSync(() => settle.current());
        await tick();
        if (after.live.regions.panel.visible) root.dataset.transicao = side();
        const { grows, shrinks } = growth(before, measure(workbench.current));
        root.dataset.cresce = grows;
        root.dataset.encolhe = shrinks;
      });
      void transition.finished.finally(() => {
        running -= 1;
        if (running === 0) for (const mark of MARKS) delete root.dataset[mark];
      });
    });
  }, []);

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

  // Uma aba aberta ou trazida para a frente no editor tira a maximização do
  // painel: a aba aparece, em vez de ficar atrás dele até ele voltar.
  useEffect(
    () =>
      useEditor.subscribe((state, previous) => {
        if (state.activeId && state.activeId !== previous.activeId && useLayout.getState().panelMaximized) {
          useLayout.getState().setPanelMaximized(false);
        }
      }),
    [],
  );

  // O duplo clique numa divisão volta a região ao tamanho da foto do layout
  // em uso. O da biblioteca voltaria ao `defaultSize`, que aqui já é o
  // tamanho de agora.
  const resetSize = (size: keyof LayoutSizes) => {
    useLayout.getState().setSizes({ [size]: snapshotSize(size) });
    enforce.current();
  };

  return (
    <div className="workbench" ref={workbench}>
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
          <Group
            orientation={bottom ? 'vertical' : 'horizontal'}
            className={`workbench__center${show.panel && maximized ? ' workbench__center--maximized' : ''}`}
            onLayoutChanged={capture}
          >
            {/* Com o painel maximizado, o editor continua aqui, escondido
             * (app.css): tirá-lo da árvore recarregava a onda numa aba, que
             * perdia zoom, cursor e sinais. */}
            <Panel id="editor" minSize={bottom ? 120 : fit.editorRight}>
              <EditorArea />
            </Panel>
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
                  groupResizeBehavior="preserve-pixel-size"
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
