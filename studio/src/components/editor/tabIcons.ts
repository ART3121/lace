// O ícone de cada aba: pela extensão do arquivo ou pelo tipo da vista.

import {
  AudioWaveform,
  ChartColumn,
  CircuitBoard,
  Cpu,
  File,
  FileCode,
  FileText,
  GitCompare,
  GraduationCap,
  House,
  Info,
  Microchip,
  ScrollText,
  Settings,
  Wrench,
  type LucideIcon,
} from 'lucide-react';

import type { Tab } from '../../state/editor';
import { extension } from '../../util/paths';

export function fileIcon(path: string): LucideIcon {
  switch (extension(path)) {
    case 'v':
    case 'sv':
    case 'vh':
    case 'svh':
    case 'cmm':
    case 'cpp':
    case 'c':
    case 'h':
    case 'asm':
    case 'py':
      return FileCode;
    case 'txt':
    case 'md':
    case 'mif':
    case 'log':
      return FileText;
    case 'spf':
      return Settings;
    default:
      return File;
  }
}

export function tabIcon(tab: Tab): LucideIcon {
  switch (tab.kind) {
    case 'file':
      return fileIcon(tab.path ?? '');
    case 'welcome':
      return House;
    case 'settings':
      return Settings;
    case 'toolchain':
      return Wrench;
    case 'schematic':
      return CircuitBoard;
    case 'synthesis':
      return ChartColumn;
    case 'board':
      return Microchip;
    case 'report':
      return ScrollText;
    case 'compare':
      return GitCompare;
    case 'processor':
      return Cpu;
    case 'about':
      return Info;
    case 'wave':
      return AudioWaveform;
    case 'learn':
      return GraduationCap;
  }
}
