// Cada vista da janela: o ícone, o componente e as marcas que ela mostra na
// aba ou no ícone (o número de Problemas, o ponto de saída nova). O rótulo e
// a região padrão ficam no modelo (state/layoutModel.ts), sem React.
//
// As vistas que nasceram na barra lateral (`side`) são colunas que rolam por
// dentro, no fundo das barras laterais; as do painel (`panel`) ocupam a área
// inteira, no fundo do editor. Cada uma leva o seu fundo para a região onde
// estiver.

import {
  Activity,
  Binary,
  CircleAlert,
  CircuitBoard,
  FolderTree,
  GraduationCap,
  Hammer,
  ListChecks,
  Microchip,
  ScrollText,
  Search,
  SquareTerminal,
  Workflow,
  type LucideIcon,
} from 'lucide-react';

import { useLayout, type ViewId } from '../../state/layout';
import { ConsoleView, ProblemsView, ShellView, useProblemCount } from '../panel/PanelViews';
import { Explorer } from '../sidebar/Explorer';
import { FlowNavigator } from '../sidebar/FlowNavigator';
import { LearnPanel } from '../sidebar/LearnPanel';
import { ReportsPanel } from '../sidebar/ReportsPanel';
import { SearchPanel } from '../sidebar/SearchPanel';

export const VIEW_ICONS: Record<ViewId, LucideIcon> = {
  explorer: FolderTree,
  flow: Workflow,
  search: Search,
  reports: ScrollText,
  learn: GraduationCap,
  cmm: Hammer,
  asm: Binary,
  verilog: ListChecks,
  wave: Activity,
  prism: CircuitBoard,
  fpga: Microchip,
  problems: CircleAlert,
  terminal: SquareTerminal,
};

export type ViewKind = 'side' | 'panel';

export function viewKind(view: ViewId): ViewKind {
  return view === 'explorer' || view === 'flow' || view === 'search' || view === 'reports' || view === 'learn' ? 'side' : 'panel';
}

export function ViewContent({ view }: { view: ViewId }) {
  switch (view) {
    case 'explorer':
      return <Explorer />;
    case 'flow':
      return <FlowNavigator />;
    case 'search':
      return <SearchPanel />;
    case 'reports':
      return <ReportsPanel />;
    case 'learn':
      return <LearnPanel />;
    case 'problems':
      return <ProblemsView />;
    case 'terminal':
      return <ShellView />;
    default:
      return <ConsoleView channel={view} />;
  }
}

/**
 * As marcas de uma vista: o número de erros e avisos (em Problemas) e o
 * ponto de saída nova que ainda não foi vista. Numa aba de texto ficam ao
 * lado do nome; num ícone (`compact`), no canto dele.
 */
export function ViewBadge({ view, compact }: { view: ViewId; compact?: boolean }) {
  const unread = useLayout((s) => !!s.unread[view]);
  if (view === 'problems') return <ProblemBadge compact={compact} unread={unread} />;
  return unread ? <span className={compact ? 'view-badge view-badge--dot' : 'panel__unread'} aria-hidden /> : null;
}

function ProblemBadge({ compact, unread }: { compact?: boolean; unread: boolean }) {
  const { errors, warnings } = useProblemCount();
  const total = errors + warnings;
  if (total === 0) return unread ? <span className={compact ? 'view-badge view-badge--dot' : 'panel__unread'} aria-hidden /> : null;
  const tone = errors ? 'count--error' : 'count--warn';
  return <span className={`count ${tone}${compact ? ' view-badge view-badge--count' : ''}`}>{total}</span>;
}
