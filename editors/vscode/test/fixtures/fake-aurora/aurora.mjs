#!/usr/bin/env node
// Um aurora falso para os testes: devolve as respostas que o aurora de verdade
// deu (responses/cases.json, gravadas por scripts/record-fixtures.mjs) e as
// de responses/synthetic.json, escritas à mão para o que o aurora não faz de
// propósito (pânico, saída que não é JSON).
//
// A resposta é escolhida pelos argumentos sem o `-C <projeto>`, com o
// caminho do projeto trocado por ${ROOT}, e sai com ${ROOT} trocado de
// volta e ${HOME} pela pasta pessoal (onde fica o bundle). Variáveis:
//
//   FAKE_AURORA_LOG    acrescenta cada chamada, uma linha JSON { argv }, e
//                    { waiting: true } quando uma resposta gravada com
//                    cancelamento já espera o SIGTERM
//   FAKE_AURORA_STATE  o arquivo do estado do projeto falso, que `top` muda;
//                    sem ele, o estado é sempre "initial"

import { appendFileSync, existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, join } from 'node:path';

const responses = join(import.meta.dirname, '..', 'responses');
const read = (file) => readFileSync(join(responses, file), 'utf8');
const cases = [...JSON.parse(read('cases.json')).cases, ...JSON.parse(read('synthetic.json')).cases];

const argv = process.argv.slice(2);
if (process.env.FAKE_AURORA_LOG) {
  appendFileSync(process.env.FAKE_AURORA_LOG, `${JSON.stringify({ argv })}\n`);
}

let project;
const rest = [];
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '-C' || argv[i] === '--project') {
    project = argv[++i];
  } else {
    rest.push(argv[i]);
  }
}
// O projeto do -C; sem ele (o `aurora new`, que ainda não tem projeto), a
// pasta do --dir.
const dirAt = rest.indexOf('--dir');
const root =
  project !== undefined
    ? project.endsWith('.spf')
      ? dirname(project)
      : project
    : dirAt >= 0 && rest[dirAt + 1]
      ? rest[dirAt + 1]
      : process.cwd();
const key = rest.map((arg) =>
  arg.startsWith(root) ? `\${ROOT}${arg.slice(root.length).replaceAll('\\', '/')}` : arg,
);

const stateFile = process.env.FAKE_AURORA_STATE;
const state = stateFile && existsSync(stateFile) ? readFileSync(stateFile, 'utf8').trim() : 'initial';

const found = cases.find(
  (c) =>
    c.args.length === key.length &&
    c.args.every((arg, i) => arg === key[i]) &&
    (c.state === undefined || c.state === state) &&
    (c.project === undefined || c.project === basename(root)),
);

if (!found) {
  const message = `Fake aurora has no recorded response for: ${key.join(' ')} (state ${state})`;
  process.stdout.write(`${JSON.stringify({ error: { code: 'fake_lace', message, hint: null } })}\n`);
  process.stderr.write(`${message}\n`);
  process.exitCode = 2;
} else {
  respond(found);
}

function respond(c) {
  const isText = c.stdout?.endsWith('.txt');
  const stdout = c.stdoutText ?? (c.stdout ? read(c.stdout) : '');
  const stderr = c.stderrText ?? (c.stderr ? read(c.stderr) : '');
  const out = substitute(stdout, !isText && c.stdoutText === undefined);
  if (stderr) {
    process.stderr.write(substitute(stderr, false));
  }
  // O arquivo que o aurora de verdade criou (o modelo do `aurora add`), com o
  // conteúdo gravado: quem chama o abre em seguida.
  for (const created of c.creates ?? []) {
    const path = substitute(created.path, false);
    mkdirSync(dirname(path), { recursive: true });
    writeFileSync(path, substitute(read(created.from), created.from.endsWith('.json') || created.from.endsWith('.spf')));
  }
  // O que o aurora de verdade moveu (o `aurora move`).
  for (const moved of c.moves ?? []) {
    const to = substitute(moved.to, false);
    mkdirSync(dirname(to), { recursive: true });
    renameSync(substitute(moved.from, false), to);
  }
  if (c.cancelAt === undefined) {
    process.stdout.write(out);
    if (c.next && stateFile) {
      writeFileSync(stateFile, c.next);
    }
    // exitCode, e não exit(): no macOS o stdout num pipe é assíncrono, e
    // sair na hora cortaria a resposta.
    process.exitCode = c.exit;
    return;
  }
  // Gravada com um SIGTERM depois de cancelAt linhas: escreve essas, espera
  // o sinal como o aurora (que encerra a ferramenta e escreve o resultado) e
  // escreve o resto. No Windows o kill encerra o processo antes disso.
  const lines = out.split('\n').filter((line) => line !== '');
  const before = lines.slice(0, c.cancelAt);
  const after = lines.slice(c.cancelAt);
  // O tratador vem antes da primeira linha: quem cancela ao ler uma linha
  // não pode chegar antes dele.
  const keepAlive = setInterval(() => {}, 1000);
  process.on('SIGTERM', () => {
    clearInterval(keepAlive);
    process.stdout.write(after.map((line) => `${line}\n`).join(''));
    process.exitCode = c.exit;
  });
  process.stdout.write(before.map((line) => `${line}\n`).join(''));
  if (process.env.FAKE_AURORA_LOG) {
    appendFileSync(process.env.FAKE_AURORA_LOG, `${JSON.stringify({ waiting: true })}\n`);
  }
}

function substitute(text, json) {
  const escape = (path) => (json ? JSON.stringify(path).slice(1, -1) : path);
  return text.replaceAll('${ROOT}', escape(root)).replaceAll('${HOME}', escape(homedir()));
}
