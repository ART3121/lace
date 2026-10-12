// O SVG exportado: o desenho do módulo sem destaque, com as cores do tema
// atual escritas em cada elemento. As variáveis CSS do Studio não vão
// junto, então o arquivo abre igual no navegador, no Inkscape e num
// documento.

import { createElement } from 'react';
import { flushSync } from 'react-dom';
import { createRoot } from 'react-dom/client';

import type { Drawing } from './engine';
import { SchematicScene } from './Scene';

const SHAPE = ['fill', 'stroke', 'stroke-width', 'stroke-dasharray', 'stroke-linejoin', 'stroke-linecap', 'stroke-opacity', 'opacity'];
const TEXT = ['fill', 'font-family', 'font-size', 'font-weight', 'letter-spacing', 'text-anchor', 'dominant-baseline', 'paint-order', 'stroke', 'stroke-width', 'stroke-linejoin'];

function escapeXml(text: string): string {
  return text.replace(/[<>&"]/g, (c) => ({ '<': '&lt;', '>': '&gt;', '&': '&amp;', '"': '&quot;' })[c]!);
}

export function exportSvg(drawing: Drawing, busWidths: boolean, title: string): string {
  const { layout } = drawing;
  const host = document.createElement('div');
  host.style.cssText = 'position:fixed;left:-100000px;top:0;visibility:hidden';
  document.body.appendChild(host);
  const root = createRoot(host);
  try {
    flushSync(() =>
      root.render(
        createElement(
          'svg',
          { xmlns: 'http://www.w3.org/2000/svg' },
          createElement(SchematicScene, {
            graph: drawing.graph,
            layout,
            focus: null,
            selected: null,
            matches: new Set<string>(),
            busWidths,
          }),
        ),
      ),
    );
    const scene = host.querySelector('.sch-scene')!;
    const background = getComputedStyle(host).getPropertyValue('--sch-canvas').trim() || '#FFFFFF';
    // Os alvos de clique são transparentes e não servem fora do Studio.
    for (const hit of scene.querySelectorAll('.sch-hit, .sch-wire__hit, .sch-enter__hit')) hit.remove();
    // Primeiro calcula tudo: a cor de um filho depende da classe e do
    // data-cat do pai, que a segunda passada tira.
    const elements = [scene, ...scene.querySelectorAll<SVGElement>('*')];
    const computed = elements.map((element) => {
      const style = getComputedStyle(element);
      const properties = element.tagName === 'text' ? TEXT : element.tagName === 'g' ? [] : SHAPE;
      return properties.map((property) => [property, style.getPropertyValue(property)] as const);
    });
    elements.forEach((element, i) => {
      for (const [property, value] of computed[i]!) {
        if (value && value !== 'normal' && value !== 'none' && value !== 'auto') element.setAttribute(property, value);
        else if (value === 'none' && (property === 'fill' || property === 'stroke')) element.setAttribute(property, 'none');
      }
      for (const attribute of ['class', 'data-node', 'data-net', 'data-cat', 'data-flag', 'data-net-source']) {
        element.removeAttribute(attribute);
      }
    });
    const width = Math.ceil(layout.width);
    const height = Math.ceil(layout.height);
    return [
      '<?xml version="1.0" encoding="UTF-8"?>',
      `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}">`,
      `<title>${escapeXml(title)}</title>`,
      `<rect width="100%" height="100%" fill="${background}"/>`,
      scene.outerHTML,
      '</svg>',
      '',
    ].join('\n');
  } finally {
    root.unmount();
    host.remove();
  }
}
