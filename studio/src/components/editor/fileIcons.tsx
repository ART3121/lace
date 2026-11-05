// Os ícones de arquivo da AURORA: um desenho próprio para cada tipo do fluxo
// (Verilog, testbench, C±, assembly, MIF, onda e o projeto), na cor do tipo
// no tema (`--file-*`, themes/index.ts). Quem escolhe o tipo de um arquivo é
// o `fileKind` (tabIcons.ts); os outros arquivos usam os ícones do Lucide.
// Os desenhos seguem o grid de 24 e o traço do Lucide, e têm o tipo dele,
// para abas e explorador usarem uns e outros do mesmo jeito.

import type { LucideIcon, LucideProps } from 'lucide-react';
import { forwardRef, type ReactNode } from 'react';

export type FileKind = 'verilog' | 'testbench' | 'cmm' | 'asm' | 'mif' | 'wave' | 'project';

const GLYPHS: Record<FileKind, ReactNode> = {
  // Um módulo: a caixa, duas entradas e uma saída.
  verilog: (
    <>
      <rect x="7" y="5" width="10" height="14" rx="2" />
      <path d="M3 9h4M3 15h4M17 12h4" />
    </>
  ),
  // O frasco de laboratório: o testbench experimenta o módulo.
  testbench: (
    <>
      <path d="M10 3v6l-4.9 8.6A2.2 2.2 0 0 0 7 21h10a2.2 2.2 0 0 0 1.9-3.4L14 9V3" />
      <path d="M8.5 3h7M7.4 15h9.2" />
    </>
  ),
  // C±: o C e o mais-ou-menos.
  cmm: (
    <>
      <path d="M12.5 8.5a5 5 0 1 0 0 7" />
      <path d="M18.5 7.5v5M16 10h5M16 15.5h5" />
    </>
  ),
  // O assembly em binário, como a vista dele.
  asm: (
    <>
      <rect x="4" y="6" width="6" height="12" rx="3" />
      <path d="M14.5 8l2.5-2v12M14.5 18h5" />
    </>
  ),
  // A memória que o MIF inicializa: uma tabela de células.
  mif: (
    <>
      <rect x="3" y="6" width="18" height="12" rx="2" />
      <path d="M9 6v12M15 6v12M3 12h18" />
    </>
  ),
  // Uma onda quadrada, como a do visualizador.
  wave: <path d="M2 16h3V8h5v8h5V8h4v8h3" />,
  // O projeto: um selo com a estrela de quatro pontas.
  project: (
    <>
      <rect x="3.5" y="3.5" width="17" height="17" rx="4" />
      <path d="M12 7.5l1.3 3.2 3.2 1.3-3.2 1.3-1.3 3.2-1.3-3.2-3.2-1.3 3.2-1.3z" fill="currentColor" stroke="none" />
    </>
  ),
};

function fileGlyph(kind: FileKind): LucideIcon {
  const Glyph = forwardRef<SVGSVGElement, Omit<LucideProps, 'ref'>>(function FileGlyph(
    { size = 24, strokeWidth = 2, className, style },
    ref,
  ) {
    return (
      <svg
        ref={ref}
        xmlns="http://www.w3.org/2000/svg"
        viewBox="0 0 24 24"
        width={size}
        height={size}
        fill="none"
        stroke="currentColor"
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
        className={`file-icon file-icon--${kind}${className ? ` ${className}` : ''}`}
        style={style}
        aria-hidden="true"
      >
        {GLYPHS[kind]}
      </svg>
    );
  });
  Glyph.displayName = `FileIcon(${kind})`;
  return Glyph;
}

export const FILE_ICONS: Record<FileKind, LucideIcon> = {
  verilog: fileGlyph('verilog'),
  testbench: fileGlyph('testbench'),
  cmm: fileGlyph('cmm'),
  asm: fileGlyph('asm'),
  mif: fileGlyph('mif'),
  wave: fileGlyph('wave'),
  project: fileGlyph('project'),
};
