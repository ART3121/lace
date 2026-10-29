// As regras de nome de arquivo que decidem onde "Definir como topo" aparece.
// Repetem as do Core para o menu sumir antes de a AURORA recusar; quem decide
// continua sendo o `aurora top`. A mesma regra está, como expressão regular,
// nas cláusulas `when` do package.json, e um teste confere que as duas batem.

/**
 * O nome é de testbench pela regra que `aurora top` usa para recusar: o nome
 * sem extensão é `tb`, começa com `tb_` ou termina com `_tb`, sem
 * diferenciar caixa (`is_testbench_name`, em crates/aurora-core/src/verilog.rs).
 * `test_alu.v` pode ser topo.
 */
export function isTestbenchName(fileName: string): boolean {
  const dot = fileName.lastIndexOf('.');
  const stem = asciiLower(dot >= 0 ? fileName.slice(0, dot) : fileName);
  return stem === 'tb' || stem.startsWith('tb_') || stem.endsWith('_tb');
}

/**
 * O nome tem `tb`, `test` ou `testbench` como palavra (`and_tb.v`,
 * `test_alu.v`; `contest.v` e `latest.v` não): a regra com que `aurora add`
 * cria um testbench em vez de um módulo (`name_suggests_testbench`, em
 * crates/aurora-core/src/verilog.rs). Um arquivo que já existe é
 * classificado pelo conteúdo, e o nome só soma pontos.
 */
export function nameSuggestsTestbench(fileName: string): boolean {
  const name = asciiLower(fileName);
  for (const word of ['testbench', 'test', 'tb']) {
    for (let start = name.indexOf(word); start >= 0; start = name.indexOf(word, start + 1)) {
      const end = start + word.length;
      const before = start === 0 || !/[a-z]/.test(name[start - 1]);
      const after = end === name.length || !/[a-z]/.test(name[end]);
      if (before && after) {
        return true;
      }
    }
  }
  return false;
}

/** `.v` ou `.sv`, com essa caixa, como o Core confere. */
export function isVerilogFile(fileName: string): boolean {
  return /\.s?v$/.test(fileName);
}

// O Core usa to_ascii_lowercase: uma letra fora do ASCII não muda.
function asciiLower(text: string): string {
  return text.replace(/[A-Z]/g, (c) => c.toLowerCase());
}
