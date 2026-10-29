// Componentes pequenos usados em toda a interface: botões, campos,
// marcadores, seções recolhíveis, o menu de contexto e o símbolo do Lace.

import { Check, ChevronDown, ChevronRight, LoaderCircle } from 'lucide-react';
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

/** Um interruptor, para uma preferência que liga e desliga na hora. O nome
 * fica fora dele (uma `<label htmlFor>` com o `id`, ou `label`). */
export function Switch({
  checked,
  onChange,
  id,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  id?: string;
  /** O nome para leitores de tela, quando não há `<label>` ligada ao `id`. */
  label?: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      id={id}
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      className={`switch${checked ? ' is-on' : ''}`}
      onClick={() => onChange(!checked)}
    >
      <span className="switch__thumb" aria-hidden />
    </button>
  );
}

/** Uma escolha entre poucas opções, todas à vista. */
export function Segmented<T extends string>({
  value,
  options,
  onChange,
  label,
}: {
  value: T;
  options: { value: T; label: string }[];
  onChange: (value: T) => void;
  label: string;
}) {
  return (
    <div className="segmented" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={value === option.value}
          className={value === option.value ? 'is-active' : ''}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
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

/** O contorno do camaleão, o ícone da AURORA (o `sapho_aurora_icon.svg` da
 * AURORA Electron), com a rotação de 180 graus do original já aplicada aos
 * números. É o mesmo de public/brand/aurora-*.svg e do splash. */
export const AURORA_MARK_PATH =
  'M3930 1801l0 282l57 -7c768 -88 1469 233 1886 864c544 825 369 1947 -402 2566c-359 289 -769 434 -1223 434c-312 0 -590 -66 -873 -208c-161 -80 -371 -226 -452 -314l-22 -24l173 -173l174 -174l74 60c240 197 496 303 806 333c145 15 316 0 479 -40c563 -138 914 -496 1050 -1070c30 -130 42 -390 24 -521c-80 -553 -453 -1004 -978 -1178c-165 -55 -239 -66 -458 -66c-213 0 -264 7 -440 62c-83 26 -249 102 -323 148l-42 26l0 -315c0 -208 -3 -316 -10 -316c-21 0 -187 89 -275 147c-301 199 -532 454 -691 765l-55 108l48 0c26 0 430 -5 897 -10c888 -11 940 -10 1078 31c437 128 726 557 678 1009c-12 118 -38 210 -90 318c-50 104 -86 157 -169 244c-201 214 -502 324 -793 290c-179 -21 -380 -106 -495 -208l-48 -43l174 -174l174 -175l49 34c94 64 240 90 357 63c195 -45 350 -238 351 -436c0 -203 -128 -381 -319 -443l-70 -22l-1231 8l-1230 9l0 -31c0 -63 53 -294 101 -439c144 -433 412 -832 753 -1119c276 -233 589 -402 928 -501c118 -34 309 -75 354 -75l24 0l0 281z';

/** O quadrado justo em volta do camaleão, com ele no centro. */
export const AURORA_MARK_VIEWBOX = '1769.4 1520 4420 4420';

/**
 * O camaleão da AURORA, sem fundo, na cor de preenchimento do destaque do
 * tema (`--brand-fill`): azul no Azul, âmbar no Âmbar, a cor do texto no
 * Grafite.
 */
export function AuroraMark({ className }: { className?: string }) {
  return (
    <svg className={className ? `aurora-mark ${className}` : 'aurora-mark'} viewBox={AURORA_MARK_VIEWBOX} aria-hidden="true">
      <path d={AURORA_MARK_PATH} />
    </svg>
  );
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
  /** Um item que liga e desliga: `true` mostra a marca no lugar do ícone.
   * Ausente, o item não é desses. */
  checked?: boolean;
  disabled?: boolean;
  danger?: boolean;
  separator?: boolean;
  /** Um submenu (um nível só), aberto ao passar o mouse ou clicar. */
  submenu?: MenuItem[];
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

/**
 * Um submenu ao lado do item que o abriu. A posição é fixa, calculada pelo
 * retângulo do item (sem a Popover API nem o posicionamento por âncora do
 * CSS, que o WebKit do macOS 13 e do Linux não têm); perto da borda da tela,
 * ele abre para a esquerda. Fica dentro do menu de cima no DOM, então um
 * clique nele não conta como clique fora do menu.
 */
function Submenu({ items, anchor, onDone }: { items: MenuItem[]; anchor: DOMRect; onDone: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ x: anchor.right - 2, y: anchor.top - 5 });
  useLayoutEffect(() => {
    if (!ref.current) return;
    const rect = ref.current.getBoundingClientRect();
    const right = anchor.right - 2;
    const x = right + rect.width > window.innerWidth - 8 ? Math.max(8, anchor.left - rect.width + 2) : right;
    const y = Math.max(8, Math.min(anchor.top - 5, window.innerHeight - rect.height - 8));
    setPosition({ x, y });
  }, [anchor]);
  return (
    <div ref={ref} className="menu menu--floating menu--submenu" style={{ left: position.x, top: position.y }} role="menu">
      <MenuList items={items} onDone={onDone} />
    </div>
  );
}

export function MenuList({ items, onDone }: { items: MenuItem[]; onDone: () => void }) {
  const [open, setOpen] = useState<{ index: number; anchor: DOMRect } | null>(null);
  const openAt = (index: number, element: HTMLElement) => setOpen({ index, anchor: element.getBoundingClientRect() });
  const submenu = open ? items[open.index]?.submenu : undefined;
  return (
    <>
      {items.map((item, index) =>
        item.separator ? (
          <div key={`sep-${index}`} className="menu__sep" />
        ) : item.submenu ? (
          <button
            key={`${item.label}-${index}`}
            type="button"
            className={`menu__item menu__item--submenu${open?.index === index ? ' is-open' : ''}`}
            role="menuitem"
            aria-haspopup="menu"
            aria-expanded={open?.index === index}
            disabled={item.disabled}
            onMouseEnter={(e) => openAt(index, e.currentTarget)}
            onClick={(e) => openAt(index, e.currentTarget)}
          >
            <span className="menu__icon">{item.icon}</span>
            <span className="menu__label">{item.label}</span>
            <ChevronRight size={13} className="menu__chevron" />
          </button>
        ) : (
          <button
            key={`${item.label}-${index}`}
            type="button"
            className={`menu__item${item.danger ? ' is-danger' : ''}`}
            role={item.checked === undefined ? 'menuitem' : 'menuitemcheckbox'}
            aria-checked={item.checked}
            disabled={item.disabled}
            onMouseEnter={() => setOpen(null)}
            onClick={() => {
              onDone();
              item.run?.();
            }}
          >
            <span className="menu__icon">{item.icon ?? (item.checked ? <Check size={13} /> : null)}</span>
            <span className="menu__label">{item.label}</span>
            {item.keys && <span className="menu__keys">{item.keys}</span>}
          </button>
        ),
      )}
      {open && submenu && <Submenu items={submenu} anchor={open.anchor} onDone={onDone} />}
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
