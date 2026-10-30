// Os ícones da IDE. O resto do código importa daqui, nunca de um pacote:
// `import { Play } from '../icons'` desenha o Play do pacote escolhido em
// Preferências > Aparência. Os nomes são os do Lucide (lucide.ts), e cada
// pacote tem um desenho para todos eles; os ícones de arquivo da AURORA são
// desenhos próprios (files.tsx).

export * from './names';
export { FILE_ICONS, type FileKind } from './files';
export { applyIcons, ICON_PACKS, PackIcon, useIconPack, type IconPack } from './pack';
export type { IconName } from './lucide';
export type { IconComponent, IconProps } from './types';
