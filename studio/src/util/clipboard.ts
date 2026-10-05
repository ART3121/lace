// Copiar texto para a área de transferência. A API assíncrona do navegador
// pode ser recusada pelo motor de páginas (sem um gesto do usuário que ele
// reconheça, ou por política); aí vai pelo caminho antigo, um campo
// escondido e o comando "copiar".

export async function copyText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
    return;
  } catch {
    // Tenta o caminho antigo.
  }
  const field = document.createElement('textarea');
  field.value = text;
  field.setAttribute('readonly', '');
  field.style.position = 'fixed';
  field.style.opacity = '0';
  document.body.appendChild(field);
  field.select();
  const ok = document.execCommand('copy');
  field.remove();
  if (!ok) throw new Error('Could not copy to the clipboard');
}
