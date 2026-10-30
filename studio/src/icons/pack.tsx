// O pacote de ícones em uso (Preferências > Aparência, `settings.icons`) e o
// componente de cada ícone, que desenha o do pacote escolhido. O padrão é o
// Lucide; um valor que a IDE não conhece também vale o Lucide.

import { create } from 'zustand';

import { lucide, type IconName } from './lucide';
import { material } from './material';
import { phosphor } from './phosphor';
import { tabler } from './tabler';
import type { IconComponent, IconProps } from './types';

export const ICON_PACKS = ['lucide', 'phosphor', 'tabler', 'material'] as const;
export type IconPack = (typeof ICON_PACKS)[number];

const PACKS: Record<IconPack, Record<IconName, IconComponent>> = { lucide, phosphor, tabler, material };

export const useIconPack = create<{ pack: IconPack }>(() => ({ pack: 'lucide' }));

/** Troca o pacote em uso: todo ícone na tela se redesenha. */
export function applyIcons(pack: string | undefined): void {
  const next = (ICON_PACKS as readonly string[]).includes(pack ?? '') ? (pack as IconPack) : 'lucide';
  if (useIconPack.getState().pack !== next) useIconPack.setState({ pack: next });
}

/** O ícone de um pacote dado, sem olhar o escolhido: as prévias da Aparência. */
export function PackIcon({ pack, name, ...props }: IconProps & { pack: IconPack; name: IconName }) {
  const Glyph = PACKS[pack][name];
  return <Glyph {...props} />;
}

/** O componente de um ícone, que segue o pacote em uso. */
export function icon(name: IconName): IconComponent {
  function Glyph(props: IconProps) {
    const pack = useIconPack((s) => s.pack);
    const Drawn = PACKS[pack][name];
    return <Drawn {...props} />;
  }
  Glyph.displayName = name;
  return Glyph;
}
