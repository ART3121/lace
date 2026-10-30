import type { ComponentType, CSSProperties } from 'react';

/** O que a IDE passa a um ícone: o tamanho em pixels, a espessura do traço
 * (nos pacotes de traço, o Lucide e o Tabler) e classes. */
export interface IconProps {
  size?: number;
  strokeWidth?: number;
  className?: string;
  style?: CSSProperties;
}

/** Um ícone da IDE, de qualquer pacote. */
export type IconComponent = ComponentType<IconProps>;
