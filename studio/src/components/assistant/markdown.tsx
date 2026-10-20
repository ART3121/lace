// O texto do assistente na tela: um Markdown pequeno, só o que as respostas
// usam (parágrafos, blocos de código, código na linha, negrito, listas e
// títulos). O React escapa o texto; nada vira HTML.

import type { ReactNode } from 'react';

type Block = { kind: 'code'; lang: string; text: string } | { kind: 'text'; lines: string[] };

/** Separa os blocos de código (```) do resto. */
function blocks(markdown: string): Block[] {
  const out: Block[] = [];
  let text: string[] = [];
  let code: { lang: string; lines: string[] } | null = null;
  for (const line of markdown.split('\n')) {
    if (code) {
      if (line.trimStart().startsWith('```')) {
        out.push({ kind: 'code', lang: code.lang, text: code.lines.join('\n') });
        code = null;
      } else {
        code.lines.push(line);
      }
      continue;
    }
    if (line.trimStart().startsWith('```')) {
      if (text.length) out.push({ kind: 'text', lines: text });
      text = [];
      code = { lang: line.trim().slice(3).trim(), lines: [] };
      continue;
    }
    text.push(line);
  }
  if (code) out.push({ kind: 'code', lang: code.lang, text: code.lines.join('\n') });
  if (text.length) out.push({ kind: 'text', lines: text });
  return out;
}

/** `código` e **negrito** dentro de uma linha. */
function inline(text: string, key: string): ReactNode[] {
  const parts: ReactNode[] = [];
  const pattern = /(`[^`]+`|\*\*[^*]+\*\*)/g;
  let last = 0;
  let n = 0;
  for (const match of text.matchAll(pattern)) {
    const start = match.index ?? 0;
    if (start > last) parts.push(text.slice(last, start));
    const token = match[0];
    if (token.startsWith('`')) parts.push(<code key={`${key}-${n++}`}>{token.slice(1, -1)}</code>);
    else parts.push(<strong key={`${key}-${n++}`}>{token.slice(2, -2)}</strong>);
    last = start + token.length;
  }
  if (last < text.length) parts.push(text.slice(last));
  return parts;
}

function paragraphs(lines: string[], key: string): ReactNode[] {
  const out: ReactNode[] = [];
  let buffer: string[] = [];
  let list: string[] = [];
  let n = 0;
  const flushText = () => {
    if (buffer.length) {
      out.push(<p key={`${key}-p${n++}`}>{inline(buffer.join(' '), `${key}-p${n}`)}</p>);
      buffer = [];
    }
  };
  const flushList = () => {
    if (list.length) {
      out.push(
        <ul key={`${key}-l${n++}`}>
          {list.map((item, i) => (
            <li key={i}>{inline(item, `${key}-li${i}`)}</li>
          ))}
        </ul>,
      );
      list = [];
    }
  };
  for (const raw of lines) {
    const line = raw.trimEnd();
    const item = /^\s*(?:[-*]|\d+[.)])\s+(.*)$/.exec(line);
    const heading = /^(#{1,4})\s+(.*)$/.exec(line);
    if (line.trim() === '') {
      flushText();
      flushList();
    } else if (item) {
      flushText();
      list.push(item[1]);
    } else if (heading) {
      flushText();
      flushList();
      out.push(<h4 key={`${key}-h${n++}`}>{inline(heading[2], `${key}-h${n}`)}</h4>);
    } else {
      flushList();
      buffer.push(line.trim());
    }
  }
  flushText();
  flushList();
  return out;
}

/** O Markdown de uma resposta como elementos React. */
export function Markdown({ text }: { text: string }) {
  return (
    <>
      {blocks(text).map((block, i) =>
        block.kind === 'code' ? (
          <pre key={i} className="chat-code" data-lang={block.lang || undefined}>
            <code>{block.text}</code>
          </pre>
        ) : (
          paragraphs(block.lines, `b${i}`)
        ),
      )}
    </>
  );
}
