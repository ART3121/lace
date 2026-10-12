// Grava, com o lace de verdade, as respostas que o lace falso dos testes
// devolve (test/fixtures/responses/). Precisa de um lace com o bundle: o
// build do processador usa o YANC; o check e a simulação, o Icarus; o lint,
// o Verilator; e o wave abre o surfer-aurora (uma janela por onda, que o
// script fecha em seguida).
//
// O teste de contrato valida as respostas contra o docs/schema/ deste
// repositório, então grave com o lace compilado dele, apontando o bundle
// instalado:
//
//   cargo build --release -p lace-cli
//   LACE=../../target/release/lace LACE_TOOLCHAIN=~/.local/share/lace/toolchain \
//     npm run record:fixtures
//
// Roda numa cópia de test/fixtures/projeto demo, numa pasta temporária com
// espaço no nome, e grava o caminho de cada projeto como ${ROOT}, que o lace
// falso troca pelo projeto do `-C`. Cada resposta guarda o nome da pasta do
// projeto (`project`), que o lace falso confere: os testes usam uma pasta
// com o mesmo nome.

import { spawn, spawnSync } from 'node:child_process';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { homedir, tmpdir } from 'node:os';
import { basename, join } from 'node:path';

const lace = process.env.LACE ?? 'lace';
const here = import.meta.dirname;
const fixtures = join(here, '..', 'test', 'fixtures');
const out = join(fixtures, 'responses');

const version = run(['--version']).stdout.trim();
console.log(`Gravando com ${version} (${lace})`);

const tmp = mkdtempSync(join(tmpdir(), 'lace fixtures '));
const cases = [];
try {
  // O projeto demo: dois módulos e um .sv, dois testbenches, um processador
  // compilado e outro não, e dois .v fora do .spf. O build do soma registra
  // o .v e o testbench que ele gera.
  const demoRoot = join(tmp, 'projeto demo');
  cpSync(join(fixtures, 'projeto demo'), demoRoot, { recursive: true });
  const demo = { root: demoRoot, spf: join(demoRoot, 'demo.spf') };
  const at = (...parts) => join(demo.root, ...parts);
  check(run(['-C', demo.spf, 'build', '-p', 'soma']), 0);

  writeFileSync(join(out, 'version.txt'), `${version}\n`);
  cases.push({ args: ['--version'], stdout: 'version.txt', exit: 0 });

  record(demo, 'status.json', ['--json', 'status'], { schema: 'status.json', state: 'initial' });
  record(demo, 'status.events.jsonl', ['--events', 'status'], {
    schema: 'events.json',
    resultSchema: 'status.json',
    state: 'initial',
  });

  // O check: o projeto, um arquivo, com lint e um processador.
  const checkResult = { schema: 'events.json', resultSchema: 'check.json' };
  record(demo, 'check.events.jsonl', ['--events', 'check'], checkResult);
  record(demo, 'check-arquivo.events.jsonl', ['--events', 'check', at('rtl', 'somador.v')], checkResult);
  record(demo, 'check-lint.events.jsonl', ['--events', 'check', '--lint'], checkResult);
  record(demo, 'check-soma.events.jsonl', ['--events', 'check', '-p', 'soma'], checkResult);

  record(demo, 'top-testbench.json', ['--json', 'top', at('tb', 'contador_tb.v')], { schema: 'error.json' });
  // Muda o topo: daqui em diante o status é outro, e o lace falso acompanha
  // pelo estado (FAKE_LACE_STATE).
  record(demo, 'top-somador.json', ['--json', 'top', at('rtl', 'somador.v')], {
    schema: 'top.json',
    next: 'top-somador',
  });
  record(demo, 'status-top-somador.json', ['--json', 'status'], { schema: 'status.json', state: 'top-somador' });
  record(demo, 'top-contador.json', ['--json', 'top', at('rtl', 'contador.v')], {
    schema: 'top.json',
    state: 'top-somador',
    next: 'initial',
  });
  // Erro de argumento: o clap escreve no stderr, em texto, sem JSON.
  record(demo, 'top-args', ['--json', 'top', '--nao-existe'], {});

  // A simulação: o projeto, a rápida, um processador e outro testbench, que
  // passa a ser o do projeto (e volta).
  const simResult = { schema: 'events.json', resultSchema: 'sim.json' };
  record(demo, 'sim.events.jsonl', ['--events', 'sim'], simResult);
  record(demo, 'sim-rapida.events.jsonl', ['--events', 'sim', '--fast'], simResult);
  record(demo, 'sim-soma.events.jsonl', ['--events', 'sim', '-p', 'soma'], simResult);
  record(demo, 'sim-somador.events.jsonl', ['--events', 'sim', at('tb', 'somador_tb.v')], {
    ...simResult,
    state: 'initial',
    next: 'sim-somador',
  });
  record(demo, 'status-sim-somador.json', ['--json', 'status'], { schema: 'status.json', state: 'sim-somador' });
  record(demo, 'sim-contador.events.jsonl', ['--events', 'sim', at('tb', 'contador_tb.v')], {
    ...simResult,
    state: 'sim-somador',
    next: 'initial',
  });

  // A síntese: o topo sem e com o esquemático, e um processador. O lace
  // falso cria o netlist (que o RTL viewer abre) e o SVG.
  const synthResult = { schema: 'events.json', resultSchema: 'synth.json' };
  recordSynth(demo, 'synth-sem-svg', ['--events', 'synth'], synthResult);
  recordSynth(demo, 'synth', ['--events', 'synth', '--svg'], synthResult);
  recordSynth(demo, 'synth-soma', ['--events', 'synth', '-p', 'soma'], synthResult);

  // A onda: a do projeto, a de um processador e um arquivo, das simulações
  // gravadas acima.
  record(demo, 'wave.json', ['--json', 'wave'], { schema: 'wave.json' });
  record(demo, 'wave-soma.json', ['--json', 'wave', '-p', 'soma'], { schema: 'wave.json' });
  record(demo, 'wave-arquivo.json', ['--json', 'wave', at('contador_tb.fst')], { schema: 'wave.json' });

  // Pôr no projeto: um .v que já existe (muda o status), um como testbench
  // e um que não existe, que o Lace cria do modelo.
  record(demo, 'add-sobra.json', ['--json', 'add', at('rtl', 'sobra.v')], {
    schema: 'add.json',
    state: 'initial',
    next: 'added',
  });
  record(demo, 'status-added.json', ['--json', 'status'], { schema: 'status.json', state: 'added' });
  record(demo, 'add-velho-tb.json', ['--json', 'add', at('rtl', 'velho_tb.v'), '--tb'], { schema: 'add.json' });
  record(demo, 'add-novo.json', ['--json', 'add', at('rtl', 'novo.v')], { schema: 'add.json' });
  writeFileSync(join(out, 'add-novo.created.v'), readFileSync(at('rtl', 'novo.v')));
  cases.at(-1).creates = [{ path: '${ROOT}/rtl/novo.v', from: 'add-novo.created.v' }];

  // Um projeto novo, numa pasta sem .spf: o lace falso cria o .spf, e a
  // extensão passa a ver o projeto.
  const fresh = { root: join(tmp, 'pasta nova') };
  mkdirSync(fresh.root);
  recordNew(fresh, 'new-novo.json', ['--json', 'new', 'novo', '--dir', fresh.root], { schema: 'new.json' });
  const created = join(fresh.root, 'novo', 'novo.spf');
  writeFileSync(join(out, 'new-novo.created.spf'), sanitize(readFileSync(created, 'utf8'), fresh.root, true));
  cases.at(-1).creates = [{ path: '${ROOT}/novo/novo.spf', from: 'new-novo.created.spf' }];
  record({ root: join(fresh.root, 'novo'), spf: created }, 'status-novo.json', ['--json', 'status'], { schema: 'status.json' });
  recordNew(fresh, 'new-invalido.json', ['--json', 'new', '9x', '--dir', fresh.root], { schema: 'error.json' });

  // O fluxo SAPHO, cada caso numa cópia nova do demo (a pasta tem o mesmo
  // nome, que o lace falso confere). O build com erro vem antes do build
  // sem estado: o lace falso pega o primeiro que casa.
  const buildResult = { schema: 'events.json', resultSchema: 'build.json' };
  const brokenSapho = copyDemo('sapho quebrado');
  writeFileSync(
    join(brokenSapho.root, 'soma', 'Software', 'soma.cmm'),
    readFileSync(join(brokenSapho.root, 'soma', 'Software', 'soma.cmm'), 'utf8').replace('out(0, 0);', 'out(0, 0'),
  );
  record(brokenSapho, 'build-soma-erro.events.jsonl', ['--events', 'build', '-p', 'soma'], {
    ...buildResult,
    state: 'soma-quebrado',
  });
  const sapho = copyDemo('sapho');
  record(sapho, 'build-soma.events.jsonl', ['--events', 'build', '-p', 'soma'], buildResult);
  // O build de todos compila o filtro, que o status passa a mostrar.
  record(sapho, 'build.events.jsonl', ['--events', 'build'], { ...buildResult, next: 'built' });
  record(sapho, 'status-built.json', ['--json', 'status'], { schema: 'status.json', state: 'built' });
  record(sapho, 'proc-set-freq.json', ['--json', 'proc', 'set', 'soma', '--freq', '50'], { schema: 'proc.json' });
  record(sapho, 'proc-set-clocks.json', ['--json', 'proc', 'set', 'soma', '--clocks', '5000'], { schema: 'proc.json' });
  record(sapho, 'proc-set-arrays.json', ['--json', 'proc', 'set', 'soma', '--arrays', 'true'], { schema: 'proc.json' });
  record(sapho, 'proc-add-existe.json', ['--json', 'proc', 'add', 'soma', '--lang', 'cmm'], { schema: 'error.json' });
  record(sapho, 'proc-add-novo.json', ['--json', 'proc', 'add', 'novo', '--lang', 'cmm'], {
    schema: 'proc.json',
    state: 'initial',
    next: 'proc-novo',
  });
  const novoSource = join(sapho.root, 'novo', 'Software', 'novo.cmm');
  writeFileSync(join(out, 'proc-add-novo.created.cmm'), readFileSync(novoSource));
  cases.at(-1).creates = [{ path: '${ROOT}/novo/Software/novo.cmm', from: 'proc-add-novo.created.cmm' }];
  record(sapho, 'status-proc-novo.json', ['--json', 'status'], { schema: 'status.json', state: 'proc-novo' });

  // As ferramentas de arquivo, cada uma a partir do .spf de depois do build
  // do soma (que registra o .v e o testbench dele), gravado de volta
  // depois de cada caso.
  const files = copyDemo('arquivos');
  const file = (...parts) => join(files.root, ...parts);
  const builtSpf = readFileSync(files.spf);
  const restore = () => writeFileSync(files.spf, builtSpf);
  const orderResult = { schema: 'order.json', state: 'initial' };
  record(files, 'order-somador-subir.json', ['--json', 'order', file('rtl', 'somador.v'), '--before', file('rtl', 'contador.v')], {
    ...orderResult,
    next: 'order-somador',
  });
  record(files, 'status-order-somador.json', ['--json', 'status'], { schema: 'status.json', state: 'order-somador' });
  restore();
  record(files, 'order-somador-descer.json', ['--json', 'order', file('rtl', 'somador.v'), '--after', file('rtl', 'mux.sv')], orderResult);
  restore();
  record(files, 'order-mux-primeiro.json', ['--json', 'order', file('rtl', 'mux.sv'), '--first'], orderResult);
  restore();
  record(files, 'order-contador-ultimo.json', ['--json', 'order', file('rtl', 'contador.v'), '--last'], orderResult);
  restore();
  record(files, 'order-mux-antes.json', ['--json', 'order', file('rtl', 'mux.sv'), '--before', file('rtl', 'contador.v')], orderResult);
  restore();
  record(files, 'remove-somador.json', ['--json', 'remove', file('rtl', 'somador.v')], {
    schema: 'remove.json',
    state: 'initial',
    next: 'removed',
  });
  record(files, 'status-removed.json', ['--json', 'status'], { schema: 'status.json', state: 'removed' });
  restore();
  record(files, 'remove-dois.json', ['--json', 'remove', file('rtl', 'somador.v'), file('rtl', 'mux.sv')], {
    schema: 'remove.json',
  });
  restore();
  record(files, 'remove-sobra.json', ['--json', 'remove', file('rtl', 'sobra.v')], { schema: 'error.json' });
  // Renomear pelo Lace (o lace falso move o arquivo) e registrar o que o
  // explorador do VS Code já renomeou (--after): o mesmo status depois.
  record(files, 'move-somador.json', ['--json', 'move', file('rtl', 'somador.v'), file('rtl', 'adder.v')], {
    schema: 'move.json',
    state: 'initial',
    next: 'moved',
  });
  cases.at(-1).moves = [{ from: '${ROOT}/rtl/somador.v', to: '${ROOT}/rtl/adder.v' }];
  record(files, 'status-moved.json', ['--json', 'status'], { schema: 'status.json', state: 'moved' });
  renameSync(file('rtl', 'adder.v'), file('rtl', 'somador.v'));
  restore();
  renameSync(file('rtl', 'somador.v'), file('rtl', 'adder.v'));
  record(files, 'move-after-somador.json', ['--json', 'move', '--after', file('rtl', 'somador.v'), file('rtl', 'adder.v')], {
    schema: 'move.json',
    state: 'initial',
    next: 'moved',
  });
  renameSync(file('rtl', 'adder.v'), file('rtl', 'somador.v'));
  restore();
  // O --after de um arquivo que não foi movido: o Lace recusa.
  record(files, 'move-after-nao-movido.json', ['--json', 'move', '--after', file('rtl', 'mux.sv'), file('rtl', 'adder.v')], {
    schema: 'error.json',
  });

  // Fora de qualquer projeto: o erro com dica.
  const empty = { root: join(tmp, 'sem projeto') };
  mkdirSync(empty.root);
  record(empty, 'sem-projeto.json', ['--json', 'status'], { schema: 'error.json' });

  // Código 1: o check roda e falha. E a onda de um projeto sem testbench.
  const broken = newProject('erro', {
    'quebrado.v': '`timescale 1ns / 1ps\n\nmodule quebrado (input wire a, output wire y);\n    assign y = a\nendmodule\n',
  });
  record(broken, 'erro-check.json', ['--json', 'check'], { schema: 'check.json' });
  record(broken, 'erro-check.events.jsonl', ['--events', 'check'], checkResult);
  record(broken, 'erro-wave.json', ['--json', 'wave'], { schema: 'error.json' });

  // Código 130: uma simulação sem $finish, cancelada com SIGTERM.
  const endless = newProject('infinito', {
    'infinito_tb.v':
      '`timescale 1ns / 1ps\n\n// Sem $finish: roda até alguém cancelar.\nmodule infinito_tb;\n' +
      '    reg clk = 0;\n    always #5 clk = ~clk;\n    initial $display("comecou");\nendmodule\n',
  });
  await recordCancel(endless, 'infinito-sim.events.jsonl', ['--events', 'sim'], {
    schema: 'events.json',
    resultSchema: 'sim.json',
  });
  await recordCancel(endless, 'infinito-sim.json', ['--json', 'sim'], { schema: 'sim.json' });
  // Código 1 também: a simulação que passa do --timeout.
  record(endless, 'infinito-timeout.events.jsonl', ['--events', 'sim', '--timeout', '1'], {
    schema: 'events.json',
    resultSchema: 'sim.json',
  });

  const manifest = {
    note: 'Gravado por scripts/record-fixtures.mjs; não edite à mão.',
    recordedWith: version,
    cases,
  };
  writeFileSync(join(out, 'cases.json'), `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(`${cases.length} respostas em ${out}`);
} finally {
  rmSync(tmp, { recursive: true, force: true });
}

function run(args) {
  const result = spawnSync(lace, args, { encoding: 'utf8' });
  if (result.error) {
    throw result.error;
  }
  return result;
}

function check(result, code) {
  if (result.status !== code) {
    throw new Error(`lace saiu com ${result.status}, e se esperava ${code}:\n${result.stdout}\n${result.stderr}`);
  }
}

/** Uma cópia do demo, com o soma compilado, numa pasta própria e com o mesmo nome. */
function copyDemo(label) {
  const root = join(tmp, label, 'projeto demo');
  cpSync(join(fixtures, 'projeto demo'), root, { recursive: true });
  const project = { root, spf: join(root, 'demo.spf') };
  check(run(['-C', project.spf, 'build', '-p', 'soma']), 0);
  return project;
}

function newProject(name, files) {
  check(run(['new', name, '--dir', tmp]), 0);
  const root = join(tmp, name);
  for (const [file, text] of Object.entries(files)) {
    writeFileSync(join(root, file), text);
  }
  const project = { root, spf: join(root, `${name}.spf`) };
  check(run(['-C', project.spf, 'add', ...Object.keys(files).map((f) => join(root, f))]), 0);
  return project;
}

/** Roda o comando no projeto e grava stdout, stderr e o código de saída. */
function record(project, name, args, extra) {
  const result = run(['-C', project.spf ?? project.root, ...args]);
  closeSurfer(args, result);
  const entry = {
    args: args.map((a) => toPlaceholder(a, project.root)),
    project: basename(project.root),
    exit: result.status,
    ...extra,
  };
  if (result.stdout !== '') {
    writeFileSync(join(out, name), sanitize(result.stdout, project.root, true));
    entry.stdout = name;
  } else if (result.stderr.trim() !== '') {
    // Sem stdout, o stderr é a resposta (o erro de argumento). Com stdout,
    // o stderr é só log.
    const file = `${name}.stderr.txt`;
    writeFileSync(join(out, file), sanitize(result.stderr, project.root, false));
    entry.stderr = file;
  }
  cases.push(entry);
}

/** Grava o synth e os arquivos que ele deixa: o netlist e, com --svg, o esquemático. */
function recordSynth(project, name, args, extra) {
  record(project, `${name}.events.jsonl`, args, extra);
  const result = resultOf(`${name}.events.jsonl`, project.root);
  const created = [[result.synthesis.netlist, `${name}.created.json`]];
  if (result.schematic?.svg) {
    created.push([result.schematic.svg, `${name}.created.svg`]);
  }
  cases.at(-1).creates = created.map(([path, file]) => {
    // O netlist guarda o caminho de cada fonte (o atributo src do Yosys).
    writeFileSync(join(out, file), sanitize(readFileSync(path, 'utf8'), project.root, file.endsWith('.json')));
    return { path: toPlaceholder(path, project.root), from: file };
  });
}

/** O objeto da linha final de uma resposta gravada do --events, com o caminho de verdade. */
function resultOf(name, root) {
  const lines = readFileSync(join(out, name), 'utf8').trim().split('\n');
  return JSON.parse(lines.at(-1).replaceAll('${ROOT}', JSON.stringify(root).slice(1, -1))).result;
}

/** O `lace new`, sem -C: a raiz das trocas por ${ROOT} é a pasta do --dir. */
function recordNew(place, name, args, extra) {
  const result = run(args);
  const entry = {
    args: args.map((a) => toPlaceholder(a, place.root)),
    project: basename(place.root),
    exit: result.status,
    stdout: name,
    ...extra,
  };
  writeFileSync(join(out, name), sanitize(result.stdout, place.root, true));
  cases.push(entry);
}

/** O wave deixa o surfer-aurora aberto: fecha pelo PID que o JSON traz. */
function closeSurfer(args, result) {
  if (args[1] !== 'wave' || result.status !== 0) {
    return;
  }
  const { pid } = JSON.parse(result.stdout);
  try {
    process.kill(pid);
  } catch {
    // já fechou
  }
}

/**
 * Roda o comando, manda SIGTERM depois da linha em que o testbench escreve
 * (no --json, que não escreve nada antes do fim, depois de 3 s) e grava
 * quantas linhas tinham saído antes do sinal.
 */
function recordCancel(project, name, args, extra) {
  return new Promise((resolve, reject) => {
    const child = spawn(lace, ['-C', project.spf, ...args]);
    const json = args.includes('--json');
    let stdout = '';
    let cancelAt;
    const cancel = () => {
      if (cancelAt === undefined) {
        cancelAt = stdout.split('\n').filter((line) => line !== '').length;
        child.kill('SIGTERM');
      }
    };
    const timer = setTimeout(json ? cancel : () => child.kill(), json ? 3000 : 30000);
    child.stdout.setEncoding('utf8');
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
      // O evento termina em \n: quando o texto aparece, a linha inteira chegou.
      if (!json && stdout.includes('"line":"comecou"')) {
        cancel();
      }
    });
    child.on('error', reject);
    child.on('close', (code) => {
      clearTimeout(timer);
      if (code !== 130 || cancelAt === undefined) {
        reject(new Error(`${args.join(' ')}: saiu com ${code} sem ser cancelado`));
        return;
      }
      writeFileSync(join(out, name), sanitize(stdout, project.root, true));
      cases.push({ args, project: basename(project.root), stdout: name, exit: code, cancelAt, ...extra });
      resolve();
    });
  });
}

/**
 * Troca o caminho do projeto por ${ROOT}, a pasta temporária por ${TMP} e a
 * pessoal por ${HOME}, também com os espaços escapados (`\ `), como o make
 * do Verilator escreve.
 */
function sanitize(text, root, json) {
  let result = text;
  for (const [path, token] of [
    [root, '${ROOT}'],
    [tmp, '${TMP}'],
    [homedir(), '${HOME}'],
  ]) {
    for (const form of [path, path.replaceAll(' ', '\\ ')]) {
      result = result.replaceAll(json ? JSON.stringify(form).slice(1, -1) : form, token);
    }
  }
  return result;
}

function toPlaceholder(arg, root) {
  return arg.startsWith(root) ? `\${ROOT}${arg.slice(root.length).replaceAll('\\', '/')}` : arg;
}
