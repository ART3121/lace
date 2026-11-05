// Abre um VS Code de teste com a extensão, numa cópia do projeto demo, com o
// aurora falso no lugar do aurora. Roda pelo `npm run test:integration`, depois
// do `npm run build` (o VS Code carrega dist/extension.js).
//
// O aurora falso roda pelo shebang, então isto é para Linux e macOS: no
// Windows, um .mjs não executa sem shell, e a extensão nunca usa shell.
// VSCODE_TEST_VERSION escolhe o VS Code (padrão: stable).

import { cpSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { runTests } from '@vscode/test-electron';

import { EXTENSION_ROOT, FAKE_AURORA, FIXTURES } from '../support/fixtures';

/**
 * Uma rodada do VS Code de teste: `suite` escolhe os testes (index.ts) e
 * `prepare` monta a pasta que ele abre.
 */
async function runSuite(suite: 'projeto' | 'vazia', folder: string, prepare: (workspace: string) => void): Promise<void> {
  const tmp = mkdtempSync(join(tmpdir(), 'aurora vscode '));
  const workspace = join(tmp, folder);
  prepare(workspace);
  // aurora.path tem escopo de máquina: só vale nas configurações do usuário.
  // O aviso de arquivo novo fica desligado: os testes trocam o estado do
  // aurora falso, e a lista de não registrados vai e volta. A lixeira também:
  // a pasta temporária pode estar num sistema de arquivos sem ela, e o
  // VS Code então recusa apagar.
  const userData = join(tmp, 'user data');
  mkdirSync(join(userData, 'User'), { recursive: true });
  const settings = { 'aurora.path': FAKE_AURORA, 'aurora.add.askForNewFiles': false, 'files.enableTrash': false };
  writeFileSync(join(userData, 'User', 'settings.json'), JSON.stringify(settings));
  try {
    await runTests({
      version: process.env.VSCODE_TEST_VERSION ?? 'stable',
      extensionDevelopmentPath: EXTENSION_ROOT,
      extensionTestsPath: join(__dirname, 'index'),
      launchArgs: [
        workspace,
        '--disable-extensions',
        '--disable-workspace-trust',
        '--skip-welcome',
        '--skip-release-notes',
        '--user-data-dir',
        userData,
        // No Ubuntu 24.04 dos runners, o AppArmor barra o sandbox do
        // Electron baixado; o teste não precisa dele.
        ...(process.env.CI ? ['--no-sandbox'] : []),
      ],
      extensionTestsEnv: {
        FAKE_AURORA_LOG: join(tmp, 'aurora.log'),
        FAKE_AURORA_STATE: join(tmp, 'aurora.state'),
        AURORA_TEST_WORKSPACE: workspace,
        AURORA_TEST_SUITE: suite,
      },
    });
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

async function main(): Promise<void> {
  // Um projeto com fontes, testbenches e processadores.
  await runSuite('projeto', 'projeto demo', (workspace) =>
    cpSync(join(FIXTURES, 'projeto demo'), workspace, { recursive: true }),
  );
  // Uma pasta sem .spf: a extensão abre e oferece o Novo projeto. O nome é
  // o das respostas gravadas do `aurora new`.
  await runSuite('vazia', 'pasta nova', (workspace) => mkdirSync(workspace, { recursive: true }));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
