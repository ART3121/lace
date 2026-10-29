// Os botões, o indicador de espera e o menu de contexto do viewer. Quem
// abriga pode trocar pelos seus (o Studio troca, para o viewer ter a cara do
// resto dele); o VS Code e o HTML do `aurora synth --open` usam estes.

import { LoaderCircle } from 'lucide-react';
import { useEffect, useLayoutEffect, useRef, useState, type ButtonHTMLAttributes, type ComponentType } from 'react';

export interface ViewerMenuItem {
  label?: string;
  keys?: string;
  disabled?: boolean;
  separator?: boolean;
  run?: () => void;
}

export interface MenuEvent {
  clientX: number;
  clientY: number;
  preventDefault: () => void;
}

export type ButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { variant?: 'default' | 'primary'; small?: boolean };
export type IconButtonProps = ButtonHTMLAttributes<HTMLButtonElement> & { label: string; active?: boolean };

export interface RtlViewerUi {
  Button: ComponentType<ButtonProps>;
  IconButton: ComponentType<IconButtonProps>;
  Spinner: ComponentType<{ size?: number }>;
  /** Abre o menu de contexto; sem isto, o viewer desenha o dele. */
  openContextMenu?: (event: MenuEvent, items: ViewerMenuItem[]) => void;
}

function Button({ variant = 'default', small, className, children, ...rest }: ButtonProps) {
  const classes = ['rv-btn', `rv-btn--${variant}`, small ? 'rv-btn--sm' : '', className ?? ''].filter(Boolean).join(' ');
  return (
    <button type="button" className={classes} {...rest}>
      {children}
    </button>
  );
}

function IconButton({ label, active, className, children, ...rest }: IconButtonProps) {
  return (
    <button
      type="button"
      className={`rv-icon-btn${active ? ' is-active' : ''}${className ? ` ${className}` : ''}`}
      title={label}
      aria-label={label}
      {...rest}
    >
      {children}
    </button>
  );
}

function Spinner({ size = 14 }: { size?: number }) {
  return <LoaderCircle className="rv-spinner" size={size} aria-hidden />;
}

export const DEFAULT_UI: RtlViewerUi = { Button, IconButton, Spinner };

/** O menu de contexto próprio do viewer: fecha com Esc, com um clique fora ou ao escolher. */
export function ContextMenu({
  x,
  y,
  items,
  onClose,
}: {
  x: number;
  y: number;
  items: ViewerMenuItem[];
  onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [position, setPosition] = useState({ x, y });

  // Perto da borda, o menu abre para dentro da tela.
  useLayoutEffect(() => {
    const rect = ref.current?.getBoundingClientRect();
    if (!rect) return;
    setPosition({
      x: Math.max(0, Math.min(x, window.innerWidth - rect.width - 4)),
      y: Math.max(0, Math.min(y, window.innerHeight - rect.height - 4)),
    });
  }, [x, y]);

  useEffect(() => {
    const close = (event: Event) => {
      if (event instanceof KeyboardEvent && event.key !== 'Escape') return;
      if (event instanceof MouseEvent && ref.current?.contains(event.target as Node)) return;
      onClose();
    };
    window.addEventListener('mousedown', close, true);
    window.addEventListener('keydown', close, true);
    window.addEventListener('blur', onClose);
    return () => {
      window.removeEventListener('mousedown', close, true);
      window.removeEventListener('keydown', close, true);
      window.removeEventListener('blur', onClose);
    };
  }, [onClose]);

  return (
    <div ref={ref} className="rv-menu" role="menu" style={{ left: position.x, top: position.y }}>
      {items.map((item, i) =>
        item.separator ? (
          <div key={i} className="rv-menu__separator" role="separator" />
        ) : (
          <button
            key={i}
            type="button"
            role="menuitem"
            className="rv-menu__item"
            disabled={item.disabled}
            onClick={() => {
              onClose();
              item.run?.();
            }}
          >
            <span>{item.label}</span>
            {item.keys && <kbd className="rv-menu__keys">{item.keys}</kbd>}
          </button>
        ),
      )}
    </div>
  );
}
