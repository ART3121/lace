// As peças das Preferências: um grupo de linhas com título, e uma linha por
// preferência, com o nome e a descrição à esquerda e o controle à direita.
// Um caminho de pasta não cabe ao lado do nome: `stack` põe o controle
// embaixo, na largura toda.

import { useId, type ReactNode } from 'react';

export function SettingsGroup({ title, hint, children }: { title?: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <section className="settings-group">
      {(title || hint) && (
        <header className="settings-group__header">
          {title && <h3 className="settings-group__title">{title}</h3>}
          {hint && <p className="settings-group__hint">{hint}</p>}
        </header>
      )}
      <div className="settings-group__rows">{children}</div>
    </section>
  );
}

/** Uma preferência. `children` recebe o id que liga o controle ao nome:
 * clicar no nome age no controle, e o leitor de tela lê o nome. */
export function SettingRow({
  label,
  hint,
  stack,
  children,
}: {
  label: string;
  hint?: ReactNode;
  stack?: boolean;
  children: (id: string) => ReactNode;
}) {
  const id = useId();
  return (
    <div className={`setting${stack ? ' setting--stack' : ''}`}>
      <div className="setting__text">
        <label className="setting__label" htmlFor={id}>
          {label}
        </label>
        {hint && <p className="setting__hint">{hint}</p>}
      </div>
      <div className="setting__control">{children(id)}</div>
    </div>
  );
}
