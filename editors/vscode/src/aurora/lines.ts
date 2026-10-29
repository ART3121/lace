/**
 * Corta um fluxo de texto em linhas. Um pedaço do stdout pode terminar no
 * meio de uma linha, e a linha só sai quando o `\n` dela chega. Aceita
 * `\r\n`. Os bytes já chegam decodificados (`setEncoding('utf8')`), que é
 * quem cuida de um caractere partido entre dois pedaços.
 */
export class LineSplitter {
  private rest = '';

  push(chunk: string): string[] {
    const parts = (this.rest + chunk).split('\n');
    this.rest = parts.pop() ?? '';
    return parts.map(withoutCr);
  }

  /** O que sobrou depois do último `\n`, quando o fluxo acaba. */
  end(): string[] {
    const last = this.rest;
    this.rest = '';
    return last === '' ? [] : [withoutCr(last)];
  }
}

function withoutCr(line: string): string {
  return line.endsWith('\r') ? line.slice(0, -1) : line;
}
