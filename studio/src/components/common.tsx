// Componentes pequenos usados em toda a interface: botões, campos,
// marcadores, seções recolhíveis, o menu de contexto e o símbolo do Lace.

import { ChevronDown, ChevronRight, LoaderCircle } from 'lucide-react';
import {
  createContext,
  useContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type ButtonHTMLAttributes,
  type ReactNode,
} from 'react';
import { create } from 'zustand';

import { useApp } from '../state/app';

type ButtonVariant = 'default' | 'primary' | 'danger' | 'ghost';

export function Button({
  variant = 'default',
  small,
  icon,
  children,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: ButtonVariant; small?: boolean; icon?: ReactNode }) {
  const classes = ['btn', `btn--${variant}`, small ? 'btn--sm' : '', className ?? ''].filter(Boolean).join(' ');
  return (
    <button type="button" className={classes} {...rest}>
      {icon}
      {children && <span>{children}</span>}
    </button>
  );
}

export function IconButton({
  label,
  active,
  children,
  className,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & { label: string; active?: boolean }) {
  return (
    <button
      type="button"
      className={`icon-btn${active ? ' is-active' : ''}${className ? ` ${className}` : ''}`}
      title={label}
      aria-label={label}
      {...rest}
    >
      {children}
    </button>
  );
}

export function Field({
  label,
  hint,
  error,
  children,
  inline,
}: {
  label?: string;
  hint?: string;
  error?: string | null;
  children: ReactNode;
  inline?: boolean;
}) {
  return (
    <label className={`field${inline ? ' field--inline' : ''}`}>
      {label && <span className="field__label">{label}</span>}
      {children}
      {error ? <span className="field__error">{error}</span> : hint ? <span className="field__hint">{hint}</span> : null}
    </label>
  );
}

export function Checkbox({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <label className={`check${disabled ? ' is-disabled' : ''}`}>
      <input type="checkbox" checked={checked} disabled={disabled} onChange={(e) => onChange(e.target.checked)} />
      <span className="check__box" aria-hidden />
      <span>{label}</span>
    </label>
  );
}

export function Badge({
  tone = 'muted',
  children,
  title,
}: {
  tone?: 'accent' | 'ok' | 'warn' | 'error' | 'info' | 'muted';
  children: ReactNode;
  title?: string;
}) {
  return (
    <span className={`badge badge--${tone}`} title={title}>
      {children}
    </span>
  );
}

/** Um atalho em teclas: `Ctrl+Shift+P`, ou de duas etapas, `Ctrl+K Z`. */
export function Kbd({ keys }: { keys: string }) {
  return (
    <span className="kbd-group">
      {keys.split(' ').map((step, index) => (
        <span key={index} className="kbd-step">
          {step.split('+').map((key) => (
            <kbd key={key} className="kbd">
              {key}
            </kbd>
          ))}
        </span>
      ))}
    </span>
  );
}

export function Spinner({ size = 14 }: { size?: number }) {
  return <LoaderCircle className="spinner" size={size} aria-hidden />;
}

export function StatusDot({ status }: { status?: 'ok' | 'failed' | 'running' }) {
  if (status === 'running') return <Spinner size={12} />;
  return <span className={`dot${status ? ` dot--${status}` : ''}`} aria-hidden />;
}

/**
 * O símbolo do Lace, sem fundo. Os arquivos ficam em public/brand/: o traço
 * azul-marinho serve ao tema claro e o creme ao escuro.
 */
export function LaceMark({ className }: { className?: string }) {
  const dark = useApp((s) => s.theme.scheme === 'dark');
  const file = dark ? 'lace-mark-reverse.svg' : 'lace-mark.svg';
  return <img src={`/brand/${file}`} alt="" className={className} />;
}

export function Empty({ icon, children }: { icon?: ReactNode; children: ReactNode }) {
  return (
    <div className="empty">
      {icon}
      <div>{children}</div>
    </div>
  );
}

/**
 * O "recolher tudo" e o "expandir tudo" de uma árvore: cada vez que
 * `version` muda, os nós que usam `useFoldable` abaixo do provedor ficam
 * `open`. Os que montam depois de um desses comandos (os filhos de um nó que
 * acabou de abrir) já nascem assim; sem nenhum (`version` 0), valem os
 * padrões de cada nó.
 */
export const FoldContext = createContext<{ version: number; open: boolean }>({ version: 0, open: true });

/** Aberto ou fechado, obedecendo ao FoldContext. */
export function useFoldable(defaultOpen: boolean): [boolean, (open: boolean) => void] {
  const fold = useContext(FoldContext);
  const [open, setOpen] = useState(() => (fold.version > 0 ? fold.open : defaultOpen));
  const seen = useRef(fold.version);
  useEffect(() => {
    if (fold.version === seen.current) return;
    seen.current = fold.version;
    setOpen(fold.open);
  }, [fold]);
  return [open, setOpen];
}

/** Uma seção com cabeçalho que recolhe, como as do explorador do VS Code. */
export function Section({
  title,
  actions,
  children,
  defaultOpen = true,
  count,
  drop,
  dropActive,
}: {
  title: string;
  actions?: ReactNode;
  children: ReactNode;
  defaultOpen?: boolean;
  count?: number;
  /** Aceita arquivos arrastados: o nome do destino (`data-drop-section`). */
  drop?: string;
  /** Um arrasto está sobre esta seção. */
  dropActive?: boolean;
}) {
  const [open, setOpen] = useFoldable(defaultOpen);
  return (
    <section className={`section${open ? ' is-open' : ''}${dropActive ? ' is-drop-target' : ''}`} data-drop-section={drop}>
      <div className="section__header">
        <button type="button" className="section__toggle" onClick={() => setOpen(!open)}>
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
          <span className="section__title">{title}</span>
          {count !== undefined && <span className="section__count">{count}</span>}
        </button>
        {actions && <div className="section__actions">{actions}</div>}
      </div>
      {open && <div className="section__body">{children}</div>}
    </section>
  );
}

// Menu de contexto ------------------------------------------------------

export interface MenuItem {
  label?: string;
  keys?: string;
  icon?: ReactNode;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  run?: () => void;
}

interface MenuState {
  menu: { x: number; y: number; items: MenuItem[] } | null;
  show: (x: number, y: number, items: MenuItem[]) => void;
  hide: () => void;
}

export const useContextMenu = create<MenuState>((set) => ({
  menu: null,
  show: (x, y, items) => set({ menu: { x, y, items } }),
  hide: () => set({ menu: null }),
}));

/** Abre um menu de contexto no ponto do evento. */
export function openContextMenu(event: { clientX: number; clientY: number; preventDefault: () => void }, items: MenuItem[]) {
  event.preventDefault();
  useContextMenu.getState().show(event.clientX, event.clientY, items);
}

export function MenuList({ items, onDone }: { items: MenuItem[]; onDone: () => void }) {
  return (
    <>
      {items.map((item, index) =>
        item.separator ? (
          <div key={`sep-${index}`} className="menu__sep" />
        ) : (
          <button
            key={`${item.label}-${index}`}
            type="button"
            className={`menu__item${item.danger ? ' is-danger' : ''}`}
            disabled={item.disabled}
            onClick={() => {
              onDone();
              item.run?.();
            }}
          >
            <span className="menu__icon">{item.icon}</span>
            <span className="menu__label">{item.label}</span>
            {item.keys && <span className="menu__keys">{item.keys}</span>}
          </button>
        ),
      )}
    </>
  );
}

export function ContextMenuHost() {
  const { menu, hide } = useContextMenu();
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ x: 0, y: 0 });

  useLayoutEffect(() => {
    if (!menu || !ref.current) return;
    const rect = ref.current.getBoundingClientRect();
    setPosition({
      x: Math.min(menu.x, window.innerWidth - rect.width - 8),
      y: Math.min(menu.y, window.innerHeight - rect.height - 8),
    });
  }, [menu]);

  useEffect(() => {
    if (!menu) return;
    const close = (e: Event) => {
      if (ref.current && e.target instanceof Node && ref.current.contains(e.target)) return;
      hide();
    };
    const onKey = (e: KeyboardEvent) => e.key === 'Escape' && hide();
    window.addEventListener('mousedown', close, true);
    window.addEventListener('blur', hide);
    window.addEventListener('keydown', onKey, true);
    return () => {
      window.removeEventListener('mousedown', close, true);
      window.removeEventListener('blur', hide);
      window.removeEventListener('keydown', onKey, true);
    };
  }, [menu, hide]);

  if (!menu) return null;
  return (
    <div ref={ref} className="menu menu--floating" style={{ left: position.x, top: position.y }} role="menu">
      <MenuList items={menu.items} onDone={hide} />
    </div>
  );
}
