// O tour pela interface (state/tour.ts). A janela escurece, e uma parte de
// cada vez fica acesa (tourSteps.ts), com um cartão ao lado, de seta
// apontada para ela, que diz o que ela faz.
//
// A parte acesa continua viva: o mouse passa pelo recorte e chega nela, e os
// passos com tarefa avançam sozinhos quando ela é feita. O resto da janela
// não responde ao mouse; um clique ali sacode o cartão. O tour fica abaixo
// dos menus e dos diálogos (z-index): o menu Fluxo ou a paleta que o usuário
// abre numa tarefa aparecem por cima do escurecimento. As setas avançam e
// voltam e Esc sai, quando o foco está no cartão ou em lugar nenhum; com um
// diálogo aberto, as teclas são dele.

import { CircleCheck, MousePointerClick, X } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react';

import { useT } from '../../i18n';
import { openDialog, useDialogs } from '../../state/dialogs';
import { useTour } from '../../state/tour';
import { Button, IconButton, Kbd } from '../common';
import { TOUR_STEPS, type TaskMemo } from './tourSteps';

export function Tour() {
  const active = useTour((s) => s.active);
  return active ? <TourLayer /> : null;
}

/** Folga do recorte em volta da parte, distância até o cartão e margem do
 * cartão até a borda da janela, em pixels. */
const PAD = 6;
const GAP = 14;
const MARGIN = 12;
/** Quanto a tarefa feita fica à vista antes do passo seguinte, e de quanto
 * em quanto tempo o tour confere a parte e a tarefa, em ms. Um intervalo, e
 * não um quadro de animação: com a janela escondida não há quadros. */
const DONE_PAUSE = 900;
const FOLLOW_MS = 50;

interface Box {
  left: number;
  top: number;
  width: number;
  height: number;
}

/** De que lado da parte o cartão fica; a seta aponta para o outro. */
type Side = 'below' | 'above' | 'right' | 'left';

interface Placement {
  left: number;
  top: number;
  side: Side | null;
  /** Onde a seta fica na borda do cartão, em pixels a partir do canto. */
  arrow: number;
}

/**
 * Onde o cartão fica: embaixo da parte, em cima, à direita ou à esquerda, o
 * primeiro lado onde ele cabe inteiro; sem parte, no meio da janela. Se não
 * cabe em lado nenhum (o editor, que ocupa quase tudo), fica no meio da
 * própria parte, sem seta.
 */
function place(target: Box | null, card: { width: number; height: number }): Placement {
  const width = window.innerWidth;
  const height = window.innerHeight;
  const clampX = (x: number) => Math.max(MARGIN, Math.min(x, width - card.width - MARGIN));
  const clampY = (y: number) => Math.max(MARGIN, Math.min(y, height - card.height - MARGIN));
  if (!target) return { left: clampX((width - card.width) / 2), top: clampY((height - card.height) / 2), side: null, arrow: 0 };
  const left = target.left - PAD;
  const top = target.top - PAD;
  const right = target.left + target.width + PAD;
  const bottom = target.top + target.height + PAD;
  const centerX = (left + right) / 2;
  const centerY = (top + bottom) / 2;
  const middleX = clampX(centerX - card.width / 2);
  const middleY = clampY(centerY - card.height / 2);
  const sides: { side: Side; fits: boolean; left: number; top: number }[] = [
    { side: 'below', fits: bottom + GAP + card.height <= height - MARGIN, left: middleX, top: bottom + GAP },
    { side: 'above', fits: top - GAP - card.height >= MARGIN, left: middleX, top: top - GAP - card.height },
    { side: 'right', fits: right + GAP + card.width <= width - MARGIN, left: right + GAP, top: middleY },
    { side: 'left', fits: left - GAP - card.width >= MARGIN, left: left - GAP - card.width, top: middleY },
  ];
  const chosen = sides.find((candidate) => candidate.fits);
  if (!chosen) return { left: middleX, top: middleY, side: null, arrow: 0 };
  // A seta mira o meio da parte, sem sair dos cantos arredondados do cartão.
  const along = chosen.side === 'below' || chosen.side === 'above' ? centerX - chosen.left : centerY - chosen.top;
  const extent = chosen.side === 'below' || chosen.side === 'above' ? card.width : card.height;
  return { left: chosen.left, top: chosen.top, side: chosen.side, arrow: Math.max(20, Math.min(along, extent - 20)) };
}

function boxOf(element: Element | null): Box | null {
  if (!element) return null;
  const rect = element.getBoundingClientRect();
  return { left: Math.round(rect.left), top: Math.round(rect.top), width: Math.round(rect.width), height: Math.round(rect.height) };
}

const sameBox = (a: Box | null, b: Box | null) =>
  a === b || (!!a && !!b && a.left === b.left && a.top === b.top && a.width === b.width && a.height === b.height);

const ARROW_CLASS: Record<Side, string> = { below: 'up', above: 'down', right: 'left', left: 'right' };

function TourLayer() {
  const t = useT();
  const step = useTour((s) => s.step);
  const { finish, setStep } = useTour.getState();
  // A lista sai quando o tour começa: os passos cuja parte não está na tela
  // ficam de fora, e o "3 de 12" não muda no meio do caminho.
  const [steps] = useState(() => TOUR_STEPS.filter((candidate) => candidate.shown?.() ?? true));
  const index = Math.min(step, steps.length - 1);
  const current = steps[index];
  const last = index === steps.length - 1;
  const [target, setTarget] = useState<Box | null>(null);
  const [placement, setPlacement] = useState<Placement | null>(null);
  const [viewport, setViewport] = useState(0);
  // A tarefa do passo: `null` sem tarefa (ou sem como fazê-la, ou já feita
  // quando o passo começou), `false` esperando, `true` feita.
  const [task, setTask] = useState<boolean | null>(null);
  const [nudged, setNudged] = useState(false);
  const card = useRef<HTMLDivElement>(null);

  const next = () => (last ? finish() : setStep(index + 1));
  const back = () => index > 0 && setStep(index - 1);
  const newProject = () => {
    finish();
    openDialog({ kind: 'newProject' });
  };

  // Segue a parte (uma região que abre, a janela que muda de tamanho) e
  // confere a tarefa, a cada FOLLOW_MS. A primeira medida sai antes da
  // pintura: o cartão de um passo novo não aparece, nem por um quadro, no
  // lugar do anterior.
  useLayoutEffect(() => {
    const memo: TaskMemo = {};
    let waiting = !!current.task && (current.can?.() ?? true) && !(current.done?.(memo) ?? false);
    setTask(waiting ? false : null);
    // `undefined` até a primeira medida: um passo sem parte também apaga o
    // recorte do passo anterior.
    let seen: Box | null | undefined;
    const follow = () => {
      const box = boxOf(current.target?.() ?? null);
      if (seen === undefined || !sameBox(box, seen)) {
        seen = box;
        setTarget(box);
      }
      if (waiting && current.done?.(memo)) {
        waiting = false;
        setTask(true);
      }
    };
    follow();
    const timer = window.setInterval(follow, FOLLOW_MS);
    // A janela que muda de tamanho mexe em tudo: mede na hora, sem esperar.
    window.addEventListener('resize', follow);
    return () => {
      window.clearInterval(timer);
      window.removeEventListener('resize', follow);
    };
  }, [current]);

  // A tarefa feita fica à vista um instante, e o tour segue sozinho.
  useEffect(() => {
    if (task !== true) return;
    const timer = window.setTimeout(next, DONE_PAUSE);
    return () => window.clearTimeout(timer);
  }, [task, index]);

  useEffect(() => {
    const resized = () => setViewport((n) => n + 1);
    window.addEventListener('resize', resized);
    return () => window.removeEventListener('resize', resized);
  }, []);

  // O cartão mede o próprio tamanho antes de ir para o lugar.
  useLayoutEffect(() => {
    const element = card.current;
    if (!element) return;
    const next = place(target, { width: element.offsetWidth, height: element.offsetHeight });
    setPlacement((previous) =>
      previous && previous.left === next.left && previous.top === next.top && previous.side === next.side && previous.arrow === next.arrow
        ? previous
        : next,
    );
  }, [target, index, task, viewport, t]);

  // O foco vai para o cartão a cada passo: as setas e o Esc chegam nele.
  const placed = placement !== null;
  useEffect(() => {
    if (placed) card.current?.focus({ preventScroll: true });
  }, [index, placed]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (useDialogs.getState().dialog) return;
      const at = event.target instanceof Element ? event.target : null;
      const free = at === null || at === document.body || at === document.documentElement || !!card.current?.contains(at);
      if (!free) return;
      if (event.key === 'Escape') finish();
      else if (event.key === 'ArrowRight') next();
      else if (event.key === 'ArrowLeft') back();
      else return;
      event.preventDefault();
      event.stopPropagation();
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });

  // O resto da janela não responde: o clique fora da parte acesa sacode o
  // cartão, que lembra onde está o tour.
  const blocked = (event: React.MouseEvent) => {
    event.preventDefault();
    setNudged(true);
  };

  const hole = target
    ? { left: target.left - PAD, top: target.top - PAD, right: target.left + target.width + PAD, bottom: target.top + target.height + PAD }
    : null;
  const Icon = current.icon;
  const arrowStyle: CSSProperties | undefined = placement?.side
    ? placement.side === 'below' || placement.side === 'above'
      ? { left: placement.arrow - 6 }
      : { top: placement.arrow - 6 }
    : undefined;

  return (
    <div className="tour">
      {hole ? (
        <>
          <div className="tour__blocker" style={{ left: 0, top: 0, right: 0, height: Math.max(0, hole.top) }} onMouseDown={blocked} />
          <div className="tour__blocker" style={{ left: 0, top: hole.bottom, right: 0, bottom: 0 }} onMouseDown={blocked} />
          <div className="tour__blocker" style={{ left: 0, top: hole.top, width: Math.max(0, hole.left), height: hole.bottom - hole.top }} onMouseDown={blocked} />
          <div className="tour__blocker" style={{ left: hole.right, top: hole.top, right: 0, height: hole.bottom - hole.top }} onMouseDown={blocked} />
          <div
            className={`tour__spot${task === false ? ' is-waiting' : ''}`}
            style={{ left: hole.left, top: hole.top, width: hole.right - hole.left, height: hole.bottom - hole.top }}
          />
        </>
      ) : (
        <div className="tour__blocker tour__dim" onMouseDown={blocked} />
      )}
      <div
        ref={card}
        key={index}
        tabIndex={-1}
        className={`tour__card${nudged ? ' is-nudged' : ''}`}
        role="dialog"
        aria-labelledby="tour-title"
        aria-describedby="tour-body"
        style={placement ? { left: placement.left, top: placement.top } : { opacity: 0 }}
        onAnimationEnd={(e) => e.animationName === 'tour-nudge' && setNudged(false)}
      >
        {placement?.side && <span className={`tour__arrow tour__arrow--${ARROW_CLASS[placement.side]}`} style={arrowStyle} aria-hidden />}
        <div className="tour__head">
          <span className="tour__icon" aria-hidden>
            <Icon size={17} />
          </span>
          <div className="tour__heading">
            <span className="tour__count">{t('tour.count', { n: index + 1, total: steps.length })}</span>
            <h2 id="tour-title" className="tour__title">
              {t(current.title)}
            </h2>
          </div>
          <IconButton label={t('tour.close')} className="tour__close" onClick={finish}>
            <X size={15} />
          </IconButton>
        </div>
        <p id="tour-body" className="tour__body">
          {t(current.body())}
        </p>
        {task !== null && current.task && (
          <div className={`tour__task${task ? ' is-done' : ''}`} role="status">
            {task ? <CircleCheck size={17} /> : <MousePointerClick size={17} />}
            <span>{task ? t('tour.taskDone') : t(current.task)}</span>
          </div>
        )}
        <div className="tour__progress" aria-hidden>
          <span style={{ width: `${((index + 1) / steps.length) * 100}%` }} />
        </div>
        <div className="tour__foot">
          <span className="tour__keys" aria-hidden>
            <Kbd keys="←" />
            <Kbd keys="→" />
            <Kbd keys="Esc" />
          </span>
          <div className="tour__actions">
            {index === 0 && <Button onClick={finish}>{t('tour.later')}</Button>}
            {index > 0 && <Button onClick={back}>{t('tour.back')}</Button>}
            {last && <Button onClick={newProject}>{t('tour.newProject')}</Button>}
            {task === false ? (
              <Button onClick={next}>{t('tour.skip')}</Button>
            ) : (
              <Button variant="primary" onClick={next}>
                {t(last ? 'tour.finish' : index === 0 ? 'tour.start' : 'tour.next')}
              </Button>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
