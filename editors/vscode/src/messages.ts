import type { ErrorInfo } from './aurora/aurora-types';

/**
 * Um erro da AURORA numa linha, com a dica depois da mensagem. As duas vêm em
 * inglês, como a CLI as escreve. A notificação do VS Code não quebra linha.
 */
export function formatAuroraError(error: ErrorInfo): string {
  const message = error.message.trim();
  if (!error.hint) {
    return message;
  }
  return `${/[.!?:]$/.test(message) ? message : `${message}.`} ${error.hint}`;
}

/** Uma duração em milissegundos, como a CLI mostra: "383 ms", "1,2 s". */
export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms} ms` : `${(ms / 1000).toFixed(1).replace('.', ',')} s`;
}
