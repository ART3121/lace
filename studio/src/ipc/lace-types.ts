// GERADO por scripts/gen-lace-types.mjs a partir de lace/docs/schema/.
// Não edite à mão: rode `npm run gen:types`.
//
// São os tipos do Core do Lace serializados (o mesmo JSON do `--json` da
// CLI). Os nomes seguem os tipos Rust: BuildResult, SimulationResult,
// Diagnostic, Event, RunComparison.

/* eslint-disable */
/**
 * Em qual lista do `.spf` um arquivo fica. Em JSON: `"synthesizable"` ou
 * `"testbench"`.
 *
 * [`Project::add_verilog`] decide o papel pelo conteúdo, como a AURORA
 * ([`classify`](crate::verilog::classify)); [`Project::add_file`] recebe o
 * papel de quem chama. Um arquivo fica numa lista só.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FileRole".
 */
export type FileRole = 'synthesizable' | 'testbench';
/**
 * Um passo de uma operação. Em JSON, em `snake_case` (`"pre_assemble"`).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Step".
 */
export type Step =
  | 'preprocess'
  | 'compile'
  | 'pre_assemble'
  | 'assemble'
  | 'check_syntax'
  | 'lint'
  | 'elaborate'
  | 'verilate'
  | 'simulate'
  | 'synthesize'
  | 'graph'
  | 'render'
  | 'fit'
  | 'bitstream'
  | 'timing'
  | 'program';
/**
 * Linguagem do programa de um processador. Em JSON: `"cmm"` ou `"cpp"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Language".
 */
export type Language = 'cmm' | 'cpp';
/**
 * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Status".
 */
export type Status = 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
/**
 * As ferramentas que o Lace sabe orquestrar.
 *
 * Em JSON e em [`Display`](fmt::Display), cada uma aparece pelo nome do
 * programa ([`Tool::binary_name`]).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Tool".
 */
export type Tool =
  | 'cmmcomp'
  | 'appcomp'
  | 'asmcomp'
  | 'cpppp'
  | 'cppcomp'
  | 'iverilog'
  | 'vvp'
  | 'verilator'
  | 'yosys'
  | 'dot'
  | 'surfer'
  | 'openfpgaloader'
  | 'quartus'
  | 'perl';
/**
 * Como um processo terminou.
 *
 * Em JSON: `{"kind": "exited", "value": 1}`, `{"kind": "signaled",
 * "value": 11}`, `{"kind": "exception", "value": 3221225477}`,
 * `{"kind": "cancelled"}`, `{"kind": "timed_out"}`, `{"kind": "unknown"}`.
 *
 * Um código de saída e um sinal são coisas diferentes: `msg_internal` do
 * `cppcomp` chama `abort()`, e um `asmcomp` que perde um `fopen` morre por
 * segfault. Nenhum dos dois é um "erro de compilação" do usuário.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Termination".
 */
export type Termination =
  | {
      kind: 'exited';
      value: number;
    }
  | {
      kind: 'signaled';
      value: number;
    }
  | {
      kind: 'exception';
      value: number;
    }
  | {
      kind: 'cancelled';
    }
  | {
      kind: 'timed_out';
    }
  | {
      kind: 'unknown';
    };
/**
 * Gravidade de um [`Diagnostic`]. A ordem (`Error < Warning < Info <
 * Unknown`) serve para ordenar do mais grave ao menos grave.
 *
 * Em JSON: `"error"`, `"warning"`, `"info"`, `"unknown"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Severity".
 */
export type Severity = 'error' | 'warning' | 'info' | 'unknown';
/**
 * O papel de um arquivo produzido por uma operação. Em JSON, em
 * `snake_case` (`"data_memory"`).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ArtifactKind".
 */
export type ArtifactKind =
  | 'assembly'
  | 'verilog'
  | 'data_memory'
  | 'instruction_memory'
  | 'testbench'
  | 'preprocessed_source'
  | 'program_counter_map'
  | 'source_translation'
  | 'opcode_translation'
  | 'compiler_log'
  | 'pre_assembler_log'
  | 'icarus_image'
  | 'verilated_model'
  | 'waveform'
  | 'simulation_output'
  | 'netlist'
  | 'schematic_graph'
  | 'schematic'
  | 'synthesis_statistics'
  | 'board_top'
  | 'quartus_project'
  | 'sram_object'
  | 'raw_binary'
  | 'serial_vector_format';
/**
 * O que aconteceu durante uma operação, avisado na hora em que acontece.
 *
 * Em JSON, com o tipo no campo `event`:
 *
 * ```json
 * { "event": "step_started", "step": "simulate", "tool": "vvp", "command": { "...": "..." } }
 * { "event": "output", "step": "simulate", "tool": "vvp", "stream": "stdout",
 *   "line": "t=10 y=1", "diagnostic": false }
 * { "event": "step_finished", "step": "simulate", "tool": "vvp",
 *   "termination": { "kind": "exited", "value": 0 }, "duration_ms": 12 }
 * ```
 *
 * Os mesmos dados chegam depois no resultado da operação
 * ([`StepReport`](crate::StepReport)); os eventos só os adiantam.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Event".
 */
export type Event =
  | {
      /**
       * Um passo de uma operação. Em JSON, em `snake_case` (`"pre_assemble"`).
       */
      step:
        | 'preprocess'
        | 'compile'
        | 'pre_assemble'
        | 'assemble'
        | 'check_syntax'
        | 'lint'
        | 'elaborate'
        | 'verilate'
        | 'simulate'
        | 'synthesize'
        | 'graph'
        | 'render'
        | 'fit'
        | 'bitstream'
        | 'timing'
        | 'program';
      /**
       * A ferramenta.
       */
      tool:
        | 'cmmcomp'
        | 'appcomp'
        | 'asmcomp'
        | 'cpppp'
        | 'cppcomp'
        | 'iverilog'
        | 'vvp'
        | 'verilator'
        | 'yosys'
        | 'dot'
        | 'surfer'
        | 'openfpgaloader'
        | 'quartus'
        | 'perl';
      command: Invocation2;
      event: 'step_started';
    }
  | {
      /**
       * Um passo de uma operação. Em JSON, em `snake_case` (`"pre_assemble"`).
       */
      step:
        | 'preprocess'
        | 'compile'
        | 'pre_assemble'
        | 'assemble'
        | 'check_syntax'
        | 'lint'
        | 'elaborate'
        | 'verilate'
        | 'simulate'
        | 'synthesize'
        | 'graph'
        | 'render'
        | 'fit'
        | 'bitstream'
        | 'timing'
        | 'program';
      /**
       * A ferramenta que escreveu.
       */
      tool:
        | 'cmmcomp'
        | 'appcomp'
        | 'asmcomp'
        | 'cpppp'
        | 'cppcomp'
        | 'iverilog'
        | 'vvp'
        | 'verilator'
        | 'yosys'
        | 'dot'
        | 'surfer'
        | 'openfpgaloader'
        | 'quartus'
        | 'perl';
      /**
       * stdout ou stderr.
       */
      stream: 'stdout' | 'stderr';
      /**
       * A linha, decodificada como UTF-8 (bytes inválidos viram `U+FFFD`).
       */
      line: string;
      /**
       * A linha é uma mensagem da ferramenta, que volta interpretada em
       * `diagnostics` no resultado: tudo do stderr, todo o stdout dos
       * compiladores, e no stdout de uma simulação as linhas do próprio
       * simulador (`$finish called at`, `VCD info:`, o `ERROR:` de um
       * `$error`). `false` é saída do programa, como o `$display` do
       * testbench. Decidido linha a linha: a continuação indentada de uma
       * mensagem conta como `false`.
       */
      diagnostic: boolean;
      event: 'output';
    }
  | {
      /**
       * Um passo de uma operação. Em JSON, em `snake_case` (`"pre_assemble"`).
       */
      step:
        | 'preprocess'
        | 'compile'
        | 'pre_assemble'
        | 'assemble'
        | 'check_syntax'
        | 'lint'
        | 'elaborate'
        | 'verilate'
        | 'simulate'
        | 'synthesize'
        | 'graph'
        | 'render'
        | 'fit'
        | 'bitstream'
        | 'timing'
        | 'program';
      /**
       * A ferramenta.
       */
      tool:
        | 'cmmcomp'
        | 'appcomp'
        | 'asmcomp'
        | 'cpppp'
        | 'cppcomp'
        | 'iverilog'
        | 'vvp'
        | 'verilator'
        | 'yosys'
        | 'dot'
        | 'surfer'
        | 'openfpgaloader'
        | 'quartus'
        | 'perl';
      /**
       * Como terminou.
       */
      termination:
        | {
            kind: 'exited';
            value: number;
          }
        | {
            kind: 'signaled';
            value: number;
          }
        | {
            kind: 'exception';
            value: number;
          }
        | {
            kind: 'cancelled';
          }
        | {
            kind: 'timed_out';
          }
        | {
            kind: 'unknown';
          };
      /**
       * Duração, em milissegundos.
       */
      duration_ms: number;
      event: 'step_finished';
    };
/**
 * De qual saída do processo veio uma linha. Em JSON: `"stdout"` ou
 * `"stderr"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Stream".
 */
export type Stream = 'stdout' | 'stderr';
/**
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ResultTag".
 */
export type ResultTag = 'result';
/**
 * O cabo de gravação embutido na placa.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Cable".
 */
export type Cable = 'usb-blaster' | 'usb-blasterII';
/**
 * A direção de um sinal da placa, do ponto de vista do FPGA.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SignalDirection".
 */
export type SignalDirection = 'input' | 'output';
/**
 * O padrão de I/O de um sinal.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "IoStandard".
 */
export type IoStandard = string | string[];
/**
 * De onde vem um bit de entrada.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "BitSource".
 */
export type BitSource =
  | {
      /**
       * O sinal.
       */
      signal: string;
      /**
       * O bit.
       */
      bit: number;
      /**
       * Invertido (`!`).
       */
      invert: boolean;
      kind: 'pin';
    }
  | {
      /**
       * O valor.
       */
      value: boolean;
      kind: 'constant';
    };
/**
 * O que a correção percebeu sobre a causa do erro.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Finding".
 */
export type Finding =
  | {
      kind: 'interface_changed';
    }
  | {
      /**
       * A saída.
       */
      output: string;
      kind: 'undriven_output';
    }
  | {
      kind: 'reset_only';
    };
/**
 * Como terminou a correção.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Verdict".
 */
export type Verdict =
  'solved' | 'compile_error' | 'mismatch' | 'incomplete' | 'timed_out' | 'cancelled';
/**
 * Qual simulador usar. Em JSON: `"icarus"` ou `"verilator"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Simulator".
 */
export type Simulator = 'icarus' | 'verilator';
/**
 * Uma contagem do resumo do projeto.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthesisMetric".
 */
export type SynthesisMetric =
  | 'modules'
  | 'wires'
  | 'wire_bits'
  | 'public_wires'
  | 'public_wire_bits'
  | 'memories'
  | 'memory_bits'
  | 'processes'
  | 'cells';
/**
 * O que aconteceu com um número de um relatório para o outro.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Change".
 */
export type Change =
  'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
/**
 * Quanto de uma parte um relatório tem.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Availability".
 */
export type Availability = 'available' | 'partial' | 'unavailable';
/**
 * O formato do conteúdo de uma onda: FST no Icarus, VCD no Verilator. A
 * extensão é a do formato, menos quando o testbench nomeia a onda com uma
 * expressão que o Lace não resolve: aí o Icarus grava VCD no nome que o
 * testbench der.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "WaveformFormat".
 */
export type WaveformFormat = 'vcd' | 'fst';
/**
 * Como um teste terminou. Em JSON: `"passed"`, `"failed"` ou `"skipped"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TestStatus".
 */
export type TestStatus = 'passed' | 'failed' | 'skipped';
/**
 * O tipo de um [`ProjectIssue`]. Em JSON, `snake_case`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "IssueKind".
 */
export type IssueKind =
  | 'rescued_path'
  | 'selection_not_registered'
  | 'testbench_as_top'
  | 'invalid_processor_name'
  | 'duplicate_file'
  | 'nested_project';
/**
 * Uma ferramenta em `lace tools`: onde está, ou por que não está.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ToolEntry".
 */
export type ToolEntry =
  | {
      /**
       * O executável.
       */
      path: string;
      /**
       * Vem do sistema, não do bundle (só o Perl do Verilator, no Linux e
       * no macOS).
       */
      system: boolean;
    }
  | {
      error: ErrorInfo1;
    };
/**
 * O que `lace update` fez.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "UpdateAction".
 */
export type UpdateAction = 'checked' | 'up_to_date' | 'updated' | 'wizard_opened';
/**
 * Como `lace update` atualizou.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "UpdateMethod".
 */
export type UpdateMethod = 'components' | 'installer';
/**
 * A direção de uma porta. Em JSON, `"input"`, `"output"` ou `"inout"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "PortDirection".
 */
export type PortDirection = 'input' | 'output' | 'inout';
/**
 * O tipo de um escopo. Em JSON, `"module"`, `"generate"` ou `"block"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ScopeKind".
 */
export type ScopeKind = 'module' | 'generate' | 'block';
/**
 * O tipo de um sinal. Em JSON, `"reg"`, `"wire"`, `"integer"` ou `"real"`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SignalKind".
 */
export type SignalKind = 'reg' | 'wire' | 'integer' | 'real';

/**
 * O que [`Project::add_verilog`] fez com um arquivo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "AddedFile".
 */
export interface AddedFile {
  /**
   * Caminho absoluto.
   */
  path: string;
  /**
   * Em qual lista ficou.
   */
  role: 'synthesizable' | 'testbench';
  /**
   * O arquivo não existia e foi criado a partir do modelo.
   */
  created: boolean;
  /**
   * Virou o módulo de topo (era o primeiro sintetizável) ou o testbench
   * escolhido.
   */
  selected: boolean;
}
/**
 * O resultado de [`build`]. Ver o módulo `pipeline` para os campos comuns a
 * toda operação.
 *
 * Em JSON (resumido):
 *
 * ```json
 * { "processor": "soma", "language": "cmm", "status": "succeeded",
 *   "failed_step": null, "frequency_mhz": 100, "clocks": 2000,
 *   "steps": [ { "step": "compile", "tool": "cmmcomp", "...": "..." } ],
 *   "diagnostics": [],
 *   "artifacts": [ { "kind": "verilog", "path": "/p/soma/Hardware/soma.v",
 *                    "required": true, "fresh": true } ] }
 * ```
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "BuildResult".
 */
export interface BuildResult {
  /**
   * O processador compilado.
   */
  processor: string;
  /**
   * A linguagem do fonte, que decidiu o pipeline.
   */
  language: 'cmm' | 'cpp';
  /**
   * Como o build terminou.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * O passo que falhou ou quebrou, quando for o caso.
   */
  failed_step: Step | null;
  /**
   * A frequência usada (do `.spf` ou de [`BuildOptions`]).
   */
  frequency_mhz: number;
  /**
   * Os clocks usados (do `.spf` ou de [`BuildOptions`]).
   */
  clocks: number;
  /**
   * Um relatório por compilador executado. Em C±: `compile`,
   * `pre_assemble`, `assemble`. Em C: `preprocess` antes dos três.
   */
  steps: StepReport[];
  /**
   * Todas as mensagens de todos os passos, na ordem em que saíram. Hoje
   * os compiladores param no primeiro erro, mas a API já é uma lista.
   */
  diagnostics: Diagnostic[];
  /**
   * Obrigatórios: `Software/<nome>.asm`, `Hardware/<nome>.v`,
   * `Hardware/<nome>_data.mif`, `Hardware/<nome>_inst.mif` e o testbench
   * `Simulation/<nome>_tb.v`. Intermediários (`pc_<nome>_mem.txt`,
   * `trad_cmm.txt`, `trad_opcode.txt`, `cmm_log.txt`, `app_log.txt`,
   * `pp.cpp`), quando existem.
   */
  artifacts: Artifact[];
  /**
   * Quanto o build levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * O que aconteceu num passo: o comando exato, como terminou e tudo o que
 * escreveu. É o registro de auditoria da operação; os `diagnostics` do
 * resultado são a versão interpretada de `stdout` e `stderr`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "StepReport".
 */
export interface StepReport {
  /**
   * Um passo de uma operação. Em JSON, em `snake_case` (`"pre_assemble"`).
   */
  step:
    | 'preprocess'
    | 'compile'
    | 'pre_assemble'
    | 'assemble'
    | 'check_syntax'
    | 'lint'
    | 'elaborate'
    | 'verilate'
    | 'simulate'
    | 'synthesize'
    | 'graph'
    | 'render'
    | 'fit'
    | 'bitstream'
    | 'timing'
    | 'program';
  /**
   * A ferramenta executada. Para o executável que o Verilator gera, é
   * [`Tool::Verilator`].
   */
  tool:
    | 'cmmcomp'
    | 'appcomp'
    | 'asmcomp'
    | 'cpppp'
    | 'cppcomp'
    | 'iverilog'
    | 'vvp'
    | 'verilator'
    | 'yosys'
    | 'dot'
    | 'surfer'
    | 'openfpgaloader'
    | 'quartus'
    | 'perl';
  command: Invocation;
  /**
   * Como o processo terminou.
   */
  termination:
    | {
        kind: 'exited';
        value: number;
      }
    | {
        kind: 'signaled';
        value: number;
      }
    | {
        kind: 'exception';
        value: number;
      }
    | {
        kind: 'cancelled';
      }
    | {
        kind: 'timed_out';
      }
    | {
        kind: 'unknown';
      };
  /**
   * Tudo que o processo escreveu no stdout, decodificado como UTF-8 (bytes
   * inválidos viram `U+FFFD`). Numa simulação, inclui o `$display` do
   * testbench.
   */
  stdout: string;
  /**
   * Tudo que o processo escreveu no stderr.
   */
  stderr: string;
  /**
   * Duração, do início do processo até o fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * O que foi executado, com CWD e ambiente.
 */
export interface Invocation {
  /**
   * Caminho absoluto do executável. Nunca é procurado no `PATH`.
   */
  program: string;
  /**
   * Argumentos, na ordem. Caminhos já estão no formato que a ferramenta
   * aceita: o nativo do sistema, menos os do `iverilog` no Windows, que
   * vão com `/`.
   */
  args: string[];
  /**
   * Diretório de trabalho. Faz parte do contrato de várias ferramentas (o
   * `appcomp` e o `asmcomp` leem `app_log.txt` relativo a ele, o testbench
   * grava a onda nele), por isso nunca é herdado.
   */
  cwd: string;
  /**
   * Variáveis definidas pelo Lace, somadas às de `INHERITED_ENV`.
   */
  env: [unknown, unknown][];
  /**
   * Nomes de variáveis copiadas do ambiente do Lace, quando existirem.
   * Só os nomes aparecem no relatório.
   */
  inherit: string[];
}
/**
 * Uma mensagem de uma ferramenta, com os campos que deu para extrair.
 *
 * `file`, `line` e `column` são opcionais porque o `cmmcomp` não informa
 * arquivo nem coluna, e várias mensagens não informam nem a linha. Quando a
 * ferramenta não cita o arquivo mas o Lace sabe qual é (o `cmmcomp` compila
 * um fonte só), `file` vem preenchido pelo Lace.
 *
 * Em JSON:
 *
 * ```json
 * { "tool": "cmmcomp", "severity": "error",
 *   "message": "c'mon dude, declare the variable 'total' properly!",
 *   "file": "/p/conta/Software/conta.cmm", "line": 16, "column": null,
 *   "raw": "Error on line 16: c'mon dude, declare the variable 'total' properly!" }
 * ```
 *
 * No fluxo C, o `cppcomp` numera as linhas do arquivo pré-processado
 * (`<temp>/pp.cpp`), e é esse o `file` que aparece: o `cpppp` não emite
 * marcadores `#line` que permitam voltar ao fonte original.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Diagnostic".
 */
export interface Diagnostic {
  /**
   * A ferramenta que escreveu a mensagem.
   */
  tool:
    | 'cmmcomp'
    | 'appcomp'
    | 'asmcomp'
    | 'cpppp'
    | 'cppcomp'
    | 'iverilog'
    | 'vvp'
    | 'verilator'
    | 'yosys'
    | 'dot'
    | 'surfer'
    | 'openfpgaloader'
    | 'quartus'
    | 'perl';
  /**
   * A gravidade.
   */
  severity: 'error' | 'warning' | 'info' | 'unknown';
  /**
   * A mensagem, sem o prefixo de gravidade e de localização. Vem no idioma
   * da ferramenta (inglês: o Lace sempre pede `-en` ao YANC).
   */
  message: string;
  /**
   * O arquivo a que a mensagem se refere, como a ferramenta o escreveu
   * (normalmente absoluto, porque o Lace passa caminhos absolutos).
   */
  file: string | null;
  /**
   * Linha, a partir de 1.
   */
  line: number | null;
  /**
   * Coluna, a partir de 1 (Verilator e compilador C++).
   */
  column: number | null;
  /**
   * O texto exatamente como a ferramenta o escreveu (várias linhas quando
   * a ferramenta continua a mensagem em linhas indentadas).
   */
  raw: string;
}
/**
 * Um arquivo que a operação deveria produzir.
 *
 * `fresh` é calculado comparando o horário de modificação antes e depois da
 * operação. Um `.v` que sobrou de um build anterior aparece com
 * `fresh = false`, e não como sucesso. Artefatos obrigatórios aparecem
 * sempre; intermediários, só se existirem.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Artifact".
 */
export interface Artifact {
  /**
   * O papel do arquivo.
   */
  kind:
    | 'assembly'
    | 'verilog'
    | 'data_memory'
    | 'instruction_memory'
    | 'testbench'
    | 'preprocessed_source'
    | 'program_counter_map'
    | 'source_translation'
    | 'opcode_translation'
    | 'compiler_log'
    | 'pre_assembler_log'
    | 'icarus_image'
    | 'verilated_model'
    | 'waveform'
    | 'simulation_output'
    | 'netlist'
    | 'schematic_graph'
    | 'schematic'
    | 'synthesis_statistics'
    | 'board_top'
    | 'quartus_project'
    | 'sram_object'
    | 'raw_binary'
    | 'serial_vector_format';
  /**
   * Caminho absoluto.
   */
  path: string;
  /**
   * Sem ele a operação não está completa.
   */
  required: boolean;
  /**
   * Existe e foi escrito por esta operação (não é sobra de uma anterior).
   * O executável do Verilator, que o `make` só refaz quando o modelo
   * muda, conta como escrito quando a compilação terminou bem: ele está
   * em dia com a entrada.
   */
  fresh: boolean;
}
/**
 * Uma execução de programa, totalmente especificada: o registro exato do
 * que o Lace rodou, presente em todo [`StepReport`](crate::StepReport).
 *
 * Só o Core monta invocações; para os clientes o tipo é somente leitura.
 * Reproduzir um passo à mão é rodar `program` com `args`, dentro de `cwd`,
 * com um ambiente vazio mais `env` e as variáveis de `inherit`.
 *
 * Em JSON:
 *
 * ```json
 * { "program": "/opt/yanc/bin/cmmcomp",
 *   "args": ["-i", "soma.cmm", "-n", "soma", "-p", "/p/soma", "..."],
 *   "cwd": "/p/soma",
 *   "env": [],
 *   "inherit": [] }
 * ```
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Invocation".
 */
export interface Invocation1 {
  /**
   * Caminho absoluto do executável. Nunca é procurado no `PATH`.
   */
  program: string;
  /**
   * Argumentos, na ordem. Caminhos já estão no formato que a ferramenta
   * aceita: o nativo do sistema, menos os do `iverilog` no Windows, que
   * vão com `/`.
   */
  args: string[];
  /**
   * Diretório de trabalho. Faz parte do contrato de várias ferramentas (o
   * `appcomp` e o `asmcomp` leem `app_log.txt` relativo a ele, o testbench
   * grava a onda nele), por isso nunca é herdado.
   */
  cwd: string;
  /**
   * Variáveis definidas pelo Lace, somadas às de `INHERITED_ENV`.
   */
  env: [unknown, unknown][];
  /**
   * Nomes de variáveis copiadas do ambiente do Lace, quando existirem.
   * Só os nomes aparecem no relatório.
   */
  inherit: string[];
}
/**
 * O resultado de [`check`]. Não há artefatos: a verificação só diz se o
 * Verilog elabora, e o que os compiladores acharam.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "CheckResult".
 */
export interface CheckResult {
  /**
   * Os módulos elaborados como raiz: os do projeto (ou os do arquivo
   * pedido) e cada testbench.
   */
  targets: string[];
  /**
   * `Succeeded` se tudo elabora sem erro.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * O primeiro passo que falhou.
   */
  failed_step: Step | null;
  /**
   * Um `iverilog -t null` para o projeto, um por testbench e, com
   * `lint`, o `verilator --lint-only`.
   */
  steps: StepReport[];
  /**
   * Os erros e avisos, com arquivo e linha.
   */
  diagnostics: Diagnostic[];
  /**
   * Quanto a verificação levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * Um erro do Lace.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ErrorInfo".
 */
export interface ErrorInfo {
  /**
   * Código estável, em `snake_case` (`no_testbench`, `component_missing`),
   * ou `cli` para um erro da própria linha de comando.
   */
  code: string;
  /**
   * A mensagem, em inglês.
   */
  message: string;
  /**
   * O comando que resolve, quando há um.
   */
  hint?: string | null;
}
/**
 * O que vai ser executado.
 */
export interface Invocation2 {
  /**
   * Caminho absoluto do executável. Nunca é procurado no `PATH`.
   */
  program: string;
  /**
   * Argumentos, na ordem. Caminhos já estão no formato que a ferramenta
   * aceita: o nativo do sistema, menos os do `iverilog` no Windows, que
   * vão com `/`.
   */
  args: string[];
  /**
   * Diretório de trabalho. Faz parte do contrato de várias ferramentas (o
   * `appcomp` e o `asmcomp` leem `app_log.txt` relativo a ele, o testbench
   * grava a onda nele), por isso nunca é herdado.
   */
  cwd: string;
  /**
   * Variáveis definidas pelo Lace, somadas às de `INHERITED_ENV`.
   */
  env: [unknown, unknown][];
  /**
   * Nomes de variáveis copiadas do ambiente do Lace, quando existirem.
   * Só os nomes aparecem no relatório.
   */
  inherit: string[];
}
/**
 * A última linha do `--events`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ResultLine".
 */
export interface ResultLine {
  /**
   * Sempre `"result"`.
   */
  event: 'result';
  /**
   * O mesmo objeto que o comando escreve com `--json` (veja o schema do
   * comando).
   */
  result: {
    [k: string]: unknown | undefined;
  };
}
/**
 * Uma placa.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Board".
 */
export interface Board {
  /**
   * O identificador, usado no `fpga.json` e na CLI (`de2-115`).
   */
  id: string;
  /**
   * O nome para mostrar (`Terasic DE2-115`).
   */
  name: string;
  /**
   * De onde vieram os pinos: o manual, a versão e as tabelas.
   */
  source: string;
  device: Device;
  jtag: Jtag;
  /**
   * Os sinais da placa ligados ao FPGA.
   */
  signals: BoardSignal[];
}
/**
 * O FPGA.
 */
export interface Device {
  /**
   * A família, como o Quartus a escreve no `.qsf` (`Cyclone IV E`).
   */
  family: string;
  /**
   * O modelo, como no `.qsf` (`EP4CE115F29C7`).
   */
  part: string;
}
/**
 * A gravação pela USB da placa.
 */
export interface Jtag {
  /**
   * O cabo.
   */
  cable: 'usb-blaster' | 'usb-blasterII';
  /**
   * A posição do FPGA na cadeia JTAG, a partir de 1. Na DE10-Nano o FPGA
   * vem depois do processador ARM (HPS).
   */
  position: number;
  /**
   * O nome da placa no openFPGALoader (`-b`), se ele a conhece.
   */
  openfpgaloader?: string | null;
}
/**
 * Um sinal da placa: um fio (`CLOCK_50`) ou um barramento (`SW`), com um
 * pino por bit.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "BoardSignal".
 */
export interface BoardSignal {
  /**
   * O nome, como no manual (`KEY`, `LEDR`).
   */
  name: string;
  /**
   * A direção.
   */
  direction: 'input' | 'output';
  /**
   * Os pinos, do bit 0 em diante (`PIN_Y2`).
   */
  pins: string[];
  /**
   * O padrão de I/O, como no `.qsf` (`3.3-V LVTTL`): um para o sinal
   * inteiro ou um por bit, quando os bits ficam em bancos de tensões
   * diferentes.
   */
  io_standard: string | string[];
  /**
   * Ativo em nível baixo: o botão apertado lê 0, o segmento acende em 0.
   * Num sinal de saída, os bits que nada liga ficam no nível inativo.
   */
  active_low: boolean;
  /**
   * A frequência, se for um clock.
   */
  clock_mhz?: number | null;
  /**
   * O que é, como no manual.
   */
  description: string;
}
/**
 * O FPGA de uma placa.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Device".
 */
export interface Device1 {
  /**
   * A família, como o Quartus a escreve no `.qsf` (`Cyclone IV E`).
   */
  family: string;
  /**
   * O modelo, como no `.qsf` (`EP4CE115F29C7`).
   */
  part: string;
}
/**
 * Como gravar a placa pela USB dela.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Jtag".
 */
export interface Jtag1 {
  /**
   * O cabo.
   */
  cable: 'usb-blaster' | 'usb-blasterII';
  /**
   * A posição do FPGA na cadeia JTAG, a partir de 1. Na DE10-Nano o FPGA
   * vem depois do processador ARM (HPS).
   */
  position: number;
  /**
   * O nome da placa no openFPGALoader (`-b`), se ele a conhece.
   */
  openfpgaloader?: string | null;
}
/**
 * O resultado de [`build`].
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FpgaBuildResult".
 */
export interface FpgaBuildResult {
  /**
   * A placa (o `id`: `de2-115`).
   */
  board: string;
  /**
   * O topo do projeto, que o topo da placa instancia.
   */
  top: string;
  /**
   * A pasta do projeto do Quartus ([`build_dir`]).
   */
  dir: string;
  quartus: Quartus;
  /**
   * Como a compilação terminou. Um design que não alcança o clock
   * compila do mesmo jeito: quem diz é [`FpgaBuildResult::timing`].
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * O passo que falhou.
   */
  failed_step: Step | null;
  /**
   * Até quatro passos, todos do Quartus: `synthesize` (`quartus_map`),
   * `fit`, `bitstream` (`quartus_asm`) e `timing` (`quartus_sta`).
   */
  steps: StepReport[];
  /**
   * Os erros e avisos do Quartus, sem os `Info` e sem repetição (o aviso
   * de tempo vem uma vez por canto analisado).
   */
  diagnostics: Diagnostic[];
  /**
   * O topo da placa, o `.qsf` e os arquivos de gravação: o `.sof`,
   * obrigatório, e o `.rbf` e o `.svf`, para o openFPGALoader.
   */
  artifacts: Artifact[];
  /**
   * O `.sof`, quando a compilação terminou.
   */
  bitstream: string | null;
  /**
   * O que o design usa da FPGA, do resumo do Fitter, quando ele
   * terminou.
   */
  resources: ResourceUsage[];
  /**
   * A Fmax e as folgas de cada clock, quando a análise de tempo
   * terminou.
   */
  timing: TimingSummary | null;
  /**
   * Os ajustes das ligações do `fpga.json` (larguras completadas,
   * entradas soltas), como em `lace fpga check`.
   */
  notes: string[];
  /**
   * Quanto a compilação levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * O Quartus que compilou.
 */
export interface Quartus {
  /**
   * A pasta `quartus` da instalação (`C:\intelFPGA_lite\22.1std\quartus`).
   */
  root: string;
  /**
   * A pasta dos programas: `bin64` no Windows, `bin` no Linux.
   */
  bin: string;
  /**
   * A versão, pelo nome da pasta da instalação (`22.1std`).
   */
  version?: string | null;
}
/**
 * Um recurso da FPGA no resumo do Fitter.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ResourceUsage".
 */
export interface ResourceUsage {
  /**
   * O nome, como o Quartus escreve (`Total logic elements`, `Total pins`,
   * `Logic utilization (in ALMs)`).
   */
  name: string;
  /**
   * Quanto o design usa.
   */
  used: number;
  /**
   * Quanto a FPGA tem, quando o Quartus diz.
   */
  available: number | null;
  /**
   * Detalhe do recurso de cima (`Dedicated logic registers`, dentro de
   * `Total logic elements`).
   */
  detail: boolean;
}
/**
 * O tempo do design.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TimingSummary".
 */
export interface TimingSummary {
  /**
   * Um por clock, na ordem em que o relatório os cita.
   */
  clocks: ClockTiming[];
  /**
   * Nenhuma folga de setup ou de hold é negativa.
   */
  met: boolean;
}
/**
 * O tempo de um clock, no pior dos cantos de operação (tensão e
 * temperatura) que o Timing Analyzer analisa.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ClockTiming".
 */
export interface ClockTiming {
  /**
   * O clock: o sinal da placa, pelo nome do `create_clock`, ou o que o
   * Quartus achou que é clock (um registrador que dirige um `always`).
   */
  clock: string;
  /**
   * A frequência do oscilador da placa, em MHz: a que o `.sdc` pede.
   * `None` num clock que não é da placa.
   */
  target_mhz: number | null;
  /**
   * A maior frequência em que o design funciona com esse clock (a
   * `Restricted Fmax`), em MHz.
   */
  fmax_mhz: number | null;
  /**
   * A folga de setup, em ns. Negativa: o design não alcança a frequência
   * do clock.
   */
  setup_slack_ns: number | null;
  /**
   * A folga de hold, em ns.
   */
  hold_slack_ns: number | null;
}
/**
 * Uma instalação do Quartus Prime.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Quartus".
 */
export interface Quartus1 {
  /**
   * A pasta `quartus` da instalação (`C:\intelFPGA_lite\22.1std\quartus`).
   */
  root: string;
  /**
   * A pasta dos programas: `bin64` no Windows, `bin` no Linux.
   */
  bin: string;
  /**
   * A versão, pelo nome da pasta da instalação (`22.1std`).
   */
  version?: string | null;
}
/**
 * As ligações conferidas, bit a bit: o que o topo da placa precisa para ser
 * gerado.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Resolved".
 */
export interface Resolved {
  board: Board1;
  /**
   * O módulo do topo.
   */
  top: string;
  /**
   * As portas do topo, na ordem da declaração.
   */
  ports: TopPort[];
  /**
   * As entradas do topo, com a origem de cada bit, do bit 0 em diante.
   */
  inputs: PortDrive[];
  /**
   * Os sinais de saída da placa usados, com a porta que dirige cada bit.
   */
  board_outputs: SignalDrive[];
  /**
   * Os sinais de entrada da placa usados, na ordem em que aparecem.
   */
  board_inputs: string[];
  /**
   * As portas ligadas a um clock da placa.
   */
  clocks: Clock[];
  /**
   * Os ajustes feitos: larguras completadas ou cortadas, entradas soltas.
   */
  notes: string[];
}
/**
 * A placa.
 */
export interface Board1 {
  /**
   * O identificador, usado no `fpga.json` e na CLI (`de2-115`).
   */
  id: string;
  /**
   * O nome para mostrar (`Terasic DE2-115`).
   */
  name: string;
  /**
   * De onde vieram os pinos: o manual, a versão e as tabelas.
   */
  source: string;
  device: Device;
  jtag: Jtag;
  /**
   * Os sinais da placa ligados ao FPGA.
   */
  signals: BoardSignal[];
}
/**
 * Uma porta do topo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TopPort".
 */
export interface TopPort {
  /**
   * O nome.
   */
  name: string;
  /**
   * `true` para entrada, `false` para saída.
   */
  input: boolean;
  /**
   * A largura.
   */
  width: number;
}
/**
 * A origem de cada bit de uma entrada do topo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "PortDrive".
 */
export interface PortDrive {
  /**
   * A porta.
   */
  port: string;
  /**
   * Um item por bit, do bit 0 em diante.
   */
  bits: BitSource[];
}
/**
 * O que dirige cada bit de um sinal de saída da placa.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SignalDrive".
 */
export interface SignalDrive {
  /**
   * O sinal.
   */
  signal: string;
  /**
   * Ativo em nível baixo: os bits sem ligação ficam em 1.
   */
  active_low: boolean;
  /**
   * Um item por bit, do bit 0 em diante; `None` fica no nível inativo.
   */
  bits: (PortBit | null)[];
}
/**
 * Um bit de uma saída do topo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "PortBit".
 */
export interface PortBit {
  /**
   * A porta.
   */
  port: string;
  /**
   * O bit.
   */
  bit: number;
  /**
   * Invertido (`!`).
   */
  invert: boolean;
}
/**
 * Uma porta ligada a um clock da placa: vai para o `.sdc`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Clock".
 */
export interface Clock {
  /**
   * O sinal da placa.
   */
  signal: string;
  /**
   * A porta do topo.
   */
  port: string;
  /**
   * A frequência.
   */
  mhz: number;
}
/**
 * Uma ligação como texto, para mostrar.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Connection".
 */
export interface Connection {
  /**
   * A entrada do topo (`in[15:0]`) ou o sinal de saída da placa
   * (`LEDR[17:0]`).
   */
  target: string;
  /**
   * O que a dirige (`~KEY[0]`, `{10'b0, out[7:0]}`).
   */
  source: string;
}
/**
 * Uma chamada do `iverilog` e a árvore que saiu dela.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Elaboration".
 */
export interface Elaboration {
  /**
   * O testbench elaborado; `None` no design.
   */
  testbench: string | null;
  /**
   * O processador, quando o testbench é o que o build dele gerou.
   */
  processor: string | null;
  /**
   * Como esta elaboração terminou. Uma que falha não impede as outras.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * Os módulos que nenhum outro instancia, com o que eles instanciam.
   * Vazio quando a elaboração falhou.
   */
  roots: ModuleInstance[];
  /**
   * Os erros e avisos desta elaboração (também em
   * [`HierarchyResult::diagnostics`]).
   */
  diagnostics: Diagnostic[];
}
/**
 * Uma instância de módulo na hierarquia elaborada.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ModuleInstance".
 */
export interface ModuleInstance {
  /**
   * O nome da instância (`dut`). Numa raiz, o nome do módulo. Dentro de
   * blocos `generate`, `begin` ou `fork`, com o nome deles na frente,
   * separado por `.` (`op_add.my_add`): os blocos não viram nós.
   */
  name: string;
  /**
   * O módulo instanciado.
   */
  module: string;
  /**
   * O arquivo que define o módulo, absoluto.
   */
  file: string | null;
  /**
   * A linha da definição (`module ...`).
   */
  line: number | null;
  /**
   * O arquivo da instância, no módulo de cima. `None` numa raiz.
   */
  instance_file: string | null;
  /**
   * A linha da instância.
   */
  instance_line: number | null;
  /**
   * O módulo vem da biblioteca SAPHO
   * ([`Toolchain::sapho_library`](crate::Toolchain::sapho_library)).
   */
  library: boolean;
  /**
   * As instâncias de dentro, na ordem do fonte.
   */
  children: ModuleInstance[];
}
/**
 * O resultado da correção.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Grade".
 */
export interface Grade {
  /**
   * O exercício.
   */
  exercise: string;
  /**
   * Como terminou.
   */
  verdict: 'solved' | 'compile_error' | 'mismatch' | 'incomplete' | 'timed_out' | 'cancelled';
  /**
   * Quantas amostras o testbench comparou.
   */
  samples: number;
  /**
   * Em quantas amostras alguma saída errou.
   */
  mismatched: number;
  /**
   * Cada saída, na ordem das portas.
   */
  outputs: OutputCheck[];
  /**
   * Com reset: quantas amostras erradas foram com ele ativo.
   */
  reset_mismatches: number | null;
  /**
   * O que se percebeu sobre a causa do erro.
   */
  findings: Finding[];
  /**
   * Os erros e avisos dos compiladores sobre o código do aluno.
   */
  diagnostics: Diagnostic[];
  /**
   * O que o testbench escreveu, fora o resumo (um `tb.v` escrito à mão
   * pode explicar o erro aqui).
   */
  output: string[];
  /**
   * A onda da simulação.
   */
  waveform: string | null;
  /**
   * O arquivo de comandos do Surfer que mostra a onda com as entradas, as
   * saídas lado a lado com as da referência e o primeiro erro marcado.
   */
  layout: string | null;
  /**
   * Quanto a correção levou, em milissegundos.
   */
  duration_ms: number;
}
/**
 * O resultado de uma saída do módulo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "OutputCheck".
 */
export interface OutputCheck {
  /**
   * O nome da porta.
   */
  name: string;
  /**
   * Em quantas amostras ela diferiu da referência.
   */
  mismatches: number;
  /**
   * O instante do primeiro erro, em ns.
   */
  first_ns: number | null;
  /**
   * Das amostras erradas, quantas tinham bit em X ou Z.
   */
  unknown: number;
}
/**
 * Um exercício conferido.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "DevExercise".
 */
export interface DevExercise {
  /**
   * O nome.
   */
  name: string;
  /**
   * O que está errado; vazio quando está tudo certo.
   */
  problems: string[];
  /**
   * Como o `start.v` se saiu (não pode ser `solved`).
   */
  start: Verdict | null;
  /**
   * Como a `solution.v` se saiu (tem que ser `solved`).
   */
  solution: Verdict | null;
}
/**
 * Um exercício na lista, com o que o aluno já fez.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ExerciseStatus".
 */
export interface ExerciseStatus {
  /**
   * O nome.
   */
  name: string;
  /**
   * O capítulo (a pasta dele).
   */
  chapter: string;
  /**
   * O título.
   */
  title: string;
  /**
   * Já resolvido.
   */
  solved: boolean;
  /**
   * É o exercício atual.
   */
  current: boolean;
  /**
   * O arquivo que o aluno edita.
   */
  file: string;
}
/**
 * O que [`Project::move_path`] fez.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "MovedPath".
 */
export interface MovedPath {
  /**
   * Onde estava, absoluto.
   */
  from: string;
  /**
   * Onde ficou, absoluto.
   */
  to: string;
  /**
   * Os arquivos registrados no `.spf` que mudaram de lugar, já com o
   * caminho novo: o próprio arquivo, ou os que estavam dentro da pasta.
   * Continuam com o mesmo papel e a mesma marca de topo ou de testbench
   * escolhido.
   */
  files: ProjectFile[];
}
/**
 * Um arquivo registrado no projeto.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ProjectFile".
 */
export interface ProjectFile {
  /**
   * Em qual lista ele está.
   */
  role: 'synthesizable' | 'testbench';
  /**
   * Caminho absoluto.
   */
  path: string;
  /**
   * Marcado como topo (sintetizável) ou como o testbench escolhido.
   */
  top_level: boolean;
}
/**
 * O contexto de uma operação guardada.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "RunMetadata".
 */
export interface RunMetadata {
  /**
   * Quando terminou, em UTC (`2026-10-03T21:04:05Z`).
   */
  timestamp: string;
  /**
   * O comando.
   */
  command: string;
  /**
   * Como terminou.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * Quanto levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
  /**
   * A versão do Lace que gravou.
   */
  lace_version: string;
  /**
   * O que identifica o projeto: um hash da raiz dele. Relatórios de
   * projetos diferentes não se comparam.
   */
  project: string;
  /**
   * O nome do projeto.
   */
  project_name: string;
  /**
   * O bundle de ferramentas (`2026.09.29`) e a plataforma.
   */
  bundle: string;
  /**
   * `linux-x64`, `darwin-arm64`, `windows-x64`.
   */
  platform: string;
  /**
   * O módulo de topo: o da síntese, ou o testbench simulado, ou o topo do
   * projeto.
   */
  top: string | null;
  /**
   * O contexto da síntese, quando houve.
   */
  synthesis: SynthesisContext | null;
  /**
   * O contexto da simulação, quando houve.
   */
  simulation: SimulationContext | null;
  environment: Environment;
}
/**
 * O que decide se duas sínteses se comparam, e o que vira aviso.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthesisContext".
 */
export interface SynthesisContext {
  /**
   * O módulo de topo.
   */
  top: string;
  /**
   * A versão do componente yosys do bundle.
   */
  component_version: string | null;
  /**
   * Hash dos fontes sintetizáveis e dos programas dos processadores.
   */
  sources: string;
}
/**
 * O que decide se duas simulações se comparam, e o que vira aviso.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SimulationContext".
 */
export interface SimulationContext {
  /**
   * O simulador.
   */
  simulator: 'icarus' | 'verilator';
  /**
   * O testbench (o módulo de topo simulado).
   */
  testbench: string;
  /**
   * A versão do componente do simulador no bundle.
   */
  component_version: string | null;
  /**
   * Hash dos fontes, com os testbenches, e dos programas dos processadores.
   */
  sources: string;
  /**
   * Hash das entradas dos processadores (`Simulation/input_*.txt`).
   */
  inputs: string;
  /**
   * A simulação gravou onda.
   */
  waveform: boolean;
}
/**
 * A máquina.
 */
export interface Environment {
  /**
   * O nome da máquina.
   */
  hostname: string | null;
  /**
   * `linux`, `macos`, `windows`.
   */
  os: string;
  /**
   * O nome da distribuição (Linux).
   */
  os_name: string | null;
  /**
   * A versão do kernel (Linux e macOS).
   */
  kernel: string | null;
  /**
   * `x86_64`, `aarch64`.
   */
  arch: string;
  /**
   * O modelo do processador (Linux).
   */
  cpu_model: string | null;
  /**
   * Processadores lógicos disponíveis.
   */
  cpus: number | null;
  /**
   * A memória total, em bytes (Linux).
   */
  memory_bytes: number | null;
  /**
   * Hash do sistema, da arquitetura, do processador, do número de
   * processadores e do nome da máquina: muda quando a máquina muda.
   */
  fingerprint: string;
}
/**
 * A máquina onde a operação rodou. O que o sistema não informa sem rodar
 * outro programa fica `null`: o Lace só lê arquivos do sistema (no Linux,
 * `/proc` e `/etc/os-release`) e o `uname`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Environment".
 */
export interface Environment1 {
  /**
   * O nome da máquina.
   */
  hostname: string | null;
  /**
   * `linux`, `macos`, `windows`.
   */
  os: string;
  /**
   * O nome da distribuição (Linux).
   */
  os_name: string | null;
  /**
   * A versão do kernel (Linux e macOS).
   */
  kernel: string | null;
  /**
   * `x86_64`, `aarch64`.
   */
  arch: string;
  /**
   * O modelo do processador (Linux).
   */
  cpu_model: string | null;
  /**
   * Processadores lógicos disponíveis.
   */
  cpus: number | null;
  /**
   * A memória total, em bytes (Linux).
   */
  memory_bytes: number | null;
  /**
   * Hash do sistema, da arquitetura, do processador, do número de
   * processadores e do nome da máquina: muda quando a máquina muda.
   */
  fingerprint: string;
}
/**
 * As estatísticas de síntese, comparadas.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthesisComparison".
 */
export interface SynthesisComparison {
  /**
   * As contagens do resumo, na ordem do relatório.
   */
  metrics: SynthesisMetricComparison[];
  /**
   * Os tipos de célula, da maior mudança para a menor e, no empate, pelo
   * nome.
   */
  cell_types: CellComparison[];
  /**
   * Tipos que só existem no atual.
   */
  added_cell_types: number;
  /**
   * Tipos que só existem na referência.
   */
  removed_cell_types: number;
  /**
   * Contagens do resumo que aumentaram.
   */
  increased: number;
  /**
   * Que diminuíram.
   */
  decreased: number;
  /**
   * Iguais.
   */
  unchanged: number;
  /**
   * Sem um dos lados.
   */
  not_comparable: number;
}
/**
 * Uma contagem do resumo, comparada.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthesisMetricComparison".
 */
export interface SynthesisMetricComparison {
  /**
   * Qual.
   */
  metric:
    | 'modules'
    | 'wires'
    | 'wire_bits'
    | 'public_wires'
    | 'public_wire_bits'
    | 'memories'
    | 'memory_bits'
    | 'processes'
    | 'cells';
  comparison: MetricComparison;
}
/**
 * A comparação.
 */
export interface MetricComparison {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * Um tipo de célula, comparado.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "CellComparison".
 */
export interface CellComparison {
  /**
   * O tipo (`$add`).
   */
  cell_type: string;
  usage: MetricComparison1;
}
/**
 * A comparação. Um tipo que só existe de um lado conta como 0 do outro,
 * com `added` ou `removed`.
 */
export interface MetricComparison1 {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * Um número comparado.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "MetricComparison".
 */
export interface MetricComparison2 {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * Os tempos de simulação, comparados.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TimingComparison".
 */
export interface TimingComparison {
  compile: MetricComparison3;
  execution: MetricComparison4;
  total: MetricComparison5;
  simulated: MetricComparison6;
  /**
   * As duas rodaram em máquinas diferentes.
   */
  environment_changed: boolean;
}
/**
 * Compilação, em milissegundos.
 */
export interface MetricComparison3 {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * Execução, em milissegundos.
 */
export interface MetricComparison4 {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * A simulação inteira, em milissegundos.
 */
export interface MetricComparison5 {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * O tempo simulado, em femtossegundos.
 */
export interface MetricComparison6 {
  /**
   * Na referência.
   */
  baseline: number | null;
  /**
   * No atual.
   */
  current: number | null;
  /**
   * O que mudou.
   */
  change: 'unchanged' | 'increased' | 'decreased' | 'added' | 'removed' | 'not_comparable';
  /**
   * Atual menos referência.
   */
  delta: number | null;
  /**
   * A variação em porcentagem da referência. `null` quando não se
   * compara ou quando a referência é zero e o atual não (não há
   * porcentagem de zero; o texto mostra `new`).
   */
  percent: number | null;
}
/**
 * Uma linha de [`list`].
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "RunSummary".
 */
export interface RunSummary {
  /**
   * O identificador.
   */
  id: string;
  /**
   * Quando terminou; `null` num relatório ilegível.
   */
  timestamp: string | null;
  /**
   * O comando.
   */
  command: string | null;
  /**
   * Como terminou.
   */
  status: Status | null;
  /**
   * O módulo de topo.
   */
  top: string | null;
  /**
   * Tem estatísticas de síntese.
   */
  synthesis: boolean;
  /**
   * Quanto dos tempos de simulação tem.
   */
  simulation: 'available' | 'partial' | 'unavailable';
  /**
   * O `record.json` pôde ser lido.
   */
  readable: boolean;
}
/**
 * Um relatório guardado: o que a comparação e a lista usam. O texto fica ao
 * lado, em `report.txt` ([`report_text`]).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "RunRecord".
 */
export interface RunRecord {
  /**
   * O formato ([`RECORD_SCHEMA`]).
   */
  schema: number;
  /**
   * O identificador (`run-000042`).
   */
  id: string;
  metadata: RunMetadata1;
  /**
   * As estatísticas do Yosys, quando houve síntese e o Yosys as gravou.
   */
  synthesis: SynthesisStatistics | null;
  /**
   * Os tempos da simulação, quando houve simulação.
   */
  simulation: SimulationTimings | null;
}
/**
 * O contexto da operação.
 */
export interface RunMetadata1 {
  /**
   * Quando terminou, em UTC (`2026-10-03T21:04:05Z`).
   */
  timestamp: string;
  /**
   * O comando.
   */
  command: string;
  /**
   * Como terminou.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * Quanto levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
  /**
   * A versão do Lace que gravou.
   */
  lace_version: string;
  /**
   * O que identifica o projeto: um hash da raiz dele. Relatórios de
   * projetos diferentes não se comparam.
   */
  project: string;
  /**
   * O nome do projeto.
   */
  project_name: string;
  /**
   * O bundle de ferramentas (`2026.09.29`) e a plataforma.
   */
  bundle: string;
  /**
   * `linux-x64`, `darwin-arm64`, `windows-x64`.
   */
  platform: string;
  /**
   * O módulo de topo: o da síntese, ou o testbench simulado, ou o topo do
   * projeto.
   */
  top: string | null;
  /**
   * O contexto da síntese, quando houve.
   */
  synthesis: SynthesisContext | null;
  /**
   * O contexto da simulação, quando houve.
   */
  simulation: SimulationContext | null;
  environment: Environment;
}
/**
 * O que o `stat` do Yosys contou. Uma contagem que ele não informou fica
 * `null`, e é diferente de zero.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthesisStatistics".
 */
export interface SynthesisStatistics {
  /**
   * O Yosys que contou, como ele se identifica (`Yosys 0.69+156 (...)`).
   */
  tool: string;
  /**
   * O módulo de topo.
   */
  top: string;
  /**
   * Módulos.
   */
  modules: number | null;
  /**
   * Fios.
   */
  wires: number | null;
  /**
   * Bits de fio.
   */
  wire_bits: number | null;
  /**
   * Fios com nome do usuário.
   */
  public_wires: number | null;
  /**
   * Bits de fio com nome do usuário.
   */
  public_wire_bits: number | null;
  /**
   * Memórias.
   */
  memories: number | null;
  /**
   * Bits de memória.
   */
  memory_bits: number | null;
  /**
   * Processos.
   */
  processes: number | null;
  /**
   * Células, sem as instâncias de submódulo.
   */
  cells: number | null;
  /**
   * As células por tipo, da mais usada para a menos usada e, no empate,
   * pelo nome.
   */
  cell_types: CellUsage[];
}
/**
 * Quantas células de um tipo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "CellUsage".
 */
export interface CellUsage {
  /**
   * O tipo, como o Yosys o nomeia (`$add`, `$dff`).
   */
  cell_type: string;
  /**
   * Quantas.
   */
  count: number;
}
/**
 * Os tempos de uma simulação, em milissegundos de relógio. O tempo simulado
 * é outra grandeza: o quanto o tempo andou dentro do modelo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SimulationTimings".
 */
export interface SimulationTimings {
  /**
   * A simulação terminou com sucesso.
   */
  succeeded: boolean;
  /**
   * O passo que compila o Verilog (`iverilog` ou `verilator --binary`).
   */
  compile_ms: number | null;
  /**
   * O passo que roda o testbench (`vvp` ou o modelo do Verilator).
   */
  execution_ms: number | null;
  /**
   * A simulação inteira, medida à parte: com a preparação do Lace.
   */
  total_ms: number | null;
  /**
   * O tempo simulado no `$finish`, em femtossegundos, quando o simulador
   * o informa (o `vvp` informa; o Verilator, não).
   */
  simulated_fs: number | null;
}
/**
 * O resultado de [`simulate`] e de [`simulate_project`].
 *
 * Uma simulação que roda até o `$finish` e termina com código 0 é
 * `Succeeded`, mesmo que o circuito esteja errado: conferir o comportamento
 * é olhar `outputs` e a onda. O stdout do testbench (`$display`) está no
 * `stdout` do passo `simulate`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SimulationResult".
 */
export interface SimulationResult {
  /**
   * O módulo de topo simulado (o testbench).
   */
  top: string;
  /**
   * O simulador usado. Na simulação rápida de um testbench Verilog, o
   * Verilator, qualquer que fosse o pedido.
   */
  simulator: 'icarus' | 'verilator';
  /**
   * A simulação rápida ([`SimulationOptions::fast`]): rodou sem gravar
   * onda, e `waveform` é `null`.
   */
  fast: boolean;
  /**
   * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * O passo que falhou, quando for o caso: `elaborate` / `verilate`
   * (Verilog que não compila) ou `simulate` (a simulação em si).
   */
  failed_step: Step | null;
  /**
   * Icarus: `elaborate` e `simulate`. Verilator: `verilate` e `simulate`.
   */
  steps: StepReport[];
  /**
   * Mensagens de todos os passos. No passo `simulate`, só as linhas que o
   * parser reconhece: o resto do stdout é saída do testbench.
   */
  diagnostics: Diagnostic[];
  /**
   * O `.vvp` ou o executável do Verilator, a onda, e cada
   * `output_<n>.txt` escrito.
   */
  artifacts: Artifact[];
  /**
   * A onda gerada, se a simulação chegou ao fim; nunca na simulação
   * rápida.
   */
  waveform: Waveform | null;
  /**
   * `Simulation/output_<n>.txt` escritos nesta simulação.
   */
  outputs: string[];
  /**
   * Arquivos que o testbench tenta ler e que não existem. Não impedem a
   * simulação (o testbench gerado pelo `asmcomp` confere o `$fopen` antes
   * de ler), mas a porta correspondente não recebe dado nenhum.
   */
  missing_inputs: string[];
  /**
   * Os testes de um testbench cocotb (`.py`), lidos do `results.xml`;
   * `null` num testbench Verilog, ou se o cocotb não chegou a gravar o
   * resultado.
   */
  tests: TestReport | null;
  /**
   * Quanto a simulação levou, do começo ao fim (preparar, compilar e
   * rodar), em milissegundos.
   */
  duration_ms: number;
}
/**
 * Uma onda gerada por simulação, pronta para
 * [`open_waveform`](crate::open_waveform).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Waveform".
 */
export interface Waveform {
  /**
   * Caminho absoluto, com o nome que o `$dumpfile` do testbench deu.
   */
  path: string;
  /**
   * O formato do conteúdo.
   */
  format: 'vcd' | 'fst';
}
/**
 * O resultado dos testes de um testbench cocotb, lido do `results.xml`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TestReport".
 */
export interface TestReport {
  /**
   * O `results.xml` que o cocotb gravou.
   */
  results: string;
  /**
   * Os testes, na ordem em que rodaram.
   */
  cases: TestCase[];
  /**
   * Quantos passaram.
   */
  passed: number;
  /**
   * Quantos falharam.
   */
  failed: number;
  /**
   * Quantos não rodaram (`@cocotb.test(skip=True)`).
   */
  skipped: number;
}
/**
 * Um teste (`@cocotb.test()`).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TestCase".
 */
export interface TestCase {
  /**
   * `<módulo>.<teste>`, como o cocotb mostra (`test_somador.basic_test`).
   */
  name: string;
  /**
   * Como terminou.
   */
  status: 'passed' | 'failed' | 'skipped';
  /**
   * Por que falhou (a mensagem da exceção) ou não rodou; `null` quando
   * passou.
   */
  message: string | null;
  /**
   * O `.py` do teste.
   */
  file: string | null;
  /**
   * Numa falha, a linha mais funda do traceback dentro do `.py`; senão, a
   * linha do teste.
   */
  line: number | null;
}
/**
 * O que a simulação de um processador escreveu numa porta de saída.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "PortValues".
 */
export interface PortValues {
  /**
   * O número da porta.
   */
  port: number;
  /**
   * `Simulation/output_<porta>.txt`.
   */
  path: string;
  /**
   * Os valores, na ordem em que saíram. Vazio quando a porta não pôde ser
   * lida.
   */
  values: number[];
  /**
   * Por que a porta não pôde ser lida: uma linha que não é inteiro, como o
   * `x` que o simulador escreve numa divisão por zero. `null` quando leu.
   */
  error: string | null;
}
/**
 * Um processador em `lace status`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ProcessorStatus".
 */
export interface ProcessorStatus {
  /**
   * O nome: diretório, `#PRNAME`, nome do módulo Verilog e dos artefatos.
   */
  name: string;
  /**
   * A linguagem do fonte.
   */
  language: 'cmm' | 'cpp';
  /**
   * `<raiz>/<nome>`, o `-p` dos compiladores.
   */
  dir: string;
  /**
   * O programa-fonte, em `Software/`.
   */
  source: string;
  /**
   * Diretório de intermediários, o `-t` dos compiladores.
   */
  temp_dir: string;
  /**
   * Frequência de operação em MHz (`-f` do `asmcomp`).
   */
  frequency_mhz: number;
  /**
   * Clocks a simular (`-c` do `asmcomp`, vai para o testbench).
   */
  clocks: number;
  /**
   * Exporta arrays para a simulação (`-A` do `cmmcomp`).
   */
  show_arrays: boolean;
  /**
   * Já foi compilado: o Verilog e o testbench gerados existem.
   */
  built: boolean;
}
/**
 * Um aviso de [`Project::issues`]: algo estranho no `.spf` que não impede
 * de abrir.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ProjectIssue".
 */
export interface ProjectIssue {
  /**
   * O tipo, para a interface traduzir.
   */
  kind:
    | 'rescued_path'
    | 'selection_not_registered'
    | 'testbench_as_top'
    | 'invalid_processor_name'
    | 'duplicate_file'
    | 'nested_project';
  /**
   * O arquivo ou a pasta envolvida, absoluta.
   */
  path: string | null;
  /**
   * O complemento do tipo: o caminho como estava gravado
   * (`rescued_path`), o campo do `.spf` (`selection_not_registered`,
   * `testbench_as_top`) ou o nome do processador
   * (`invalid_processor_name`).
   */
  detail: string | null;
  /**
   * O aviso inteiro, em inglês, pronto para mostrar.
   */
  message: string;
}
/**
 * O resultado de [`synthesize`].
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthesisResult".
 */
export interface SynthesisResult {
  /**
   * O módulo de topo sintetizado.
   */
  top: string;
  /**
   * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * `synthesize` quando falhou.
   */
  failed_step: Step | null;
  /**
   * Um passo: o `yosys`. O log completo fica em `yosys.log`, ao lado do
   * netlist.
   */
  steps: StepReport[];
  /**
   * Os erros e avisos do Yosys, com arquivo e linha quando ele informa.
   */
  diagnostics: Diagnostic[];
  /**
   * O netlist (`hierarchy.json`).
   */
  artifacts: Artifact[];
  /**
   * `hierarchy.json`, quando a síntese terminou.
   */
  netlist: string | null;
  /**
   * Os módulos do netlist, na ordem do JSON, para escolher o que desenhar.
   */
  modules: string[];
  /**
   * O que o `stat` do Yosys contou no netlist, quando a síntese terminou
   * e o Yosys gravou o `stat.json`.
   */
  statistics: SynthesisStatistics | null;
  /**
   * Quanto a síntese levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * O resultado de [`render_schematic`].
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SchematicResult".
 */
export interface SchematicResult {
  /**
   * O módulo desenhado.
   */
  module: string;
  /**
   * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * `graph` (Yosys) ou `render` (`dot`) quando falhou.
   */
  failed_step: Step | null;
  /**
   * Dois passos: `graph` (`yosys show`) e `render` (`dot`).
   */
  steps: StepReport[];
  /**
   * Mensagens do Yosys e do Graphviz.
   */
  diagnostics: Diagnostic[];
  /**
   * O grafo (`.dot`) e o SVG.
   */
  artifacts: Artifact[];
  /**
   * O SVG, quando foi gerado. Fica ao lado do netlist, com o nome do
   * módulo (caracteres fora de `[A-Za-z0-9_.-]` viram `_`).
   */
  svg: string | null;
  /**
   * Quanto o desenho levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * Um componente instalado (`components/<nome>.json`).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "BundleComponent".
 */
export interface BundleComponent {
  /**
   * Nome (ver o módulo [`component`]); é também o nome do arquivo.
   */
  name: string;
  /**
   * Versão exata: data da release do OSS CAD Suite, tag do YANC, do
   * surfer-aurora, do Graphviz.
   */
  version: string;
  /**
   * Diretório do componente, relativo ao bundle. Componentes do OSS CAD
   * Suite dividem o mesmo.
   */
  dir: string;
  /**
   * De onde veio: URL do pacote, ou `git+<repositório>@<commit>` quando o
   * empacotamento compilou do fonte.
   */
  source: string;
  /**
   * SHA-256 do pacote de origem, quando foi baixado pronto.
   */
  sha256: string | null;
  /**
   * SHA-256 dos executáveis do componente que o Lace roda, por caminho
   * relativo ao bundle, conferido por [`Toolchain::verify`].
   */
  files: {
    [k: string]: string | undefined;
  };
}
/**
 * Por quê.
 */
export interface ErrorInfo1 {
  /**
   * Código estável, em `snake_case` (`no_testbench`, `component_missing`),
   * ou `cli` para um erro da própria linha de comando.
   */
  code: string;
  /**
   * A mensagem, em inglês.
   */
  message: string;
  /**
   * O comando que resolve, quando há um.
   */
  hint?: string | null;
}
/**
 * O compilador C++, o `make` e o Perl que o Verilator usa.
 *
 * No Windows vêm do bundle, no componente verilator (`msys/ucrt64/bin` e
 * `msys/usr/bin`), e [`SystemCompiler::bundled`] é verdadeiro. No Linux e no
 * macOS vêm do sistema, a exceção à regra do bundle, procurados em locais
 * fixos ([`SystemCompiler::detect`]), nunca no `PATH`: `/usr/bin` no Linux;
 * `/usr/bin` com as Command Line Tools do Xcode no macOS. Ou num diretório
 * declarado ([`SystemCompiler::in_dir`], e [`SystemCompiler::in_msys2`]
 * para um MSYS2 no Windows), que substitui o padrão.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SystemCompiler".
 */
export interface SystemCompiler {
  /**
   * O Perl que executa o script `verilator`.
   */
  perl: string;
  /**
   * O `make` que o Verilator chama.
   */
  make: string;
  /**
   * O compilador C++ que o `make` chama.
   */
  cxx: string;
  /**
   * Os diretórios postos no `PATH` do Verilator para ele achar `make` e o
   * compilador.
   */
  path: string[];
  /**
   * Veio do bundle (Windows), e não do sistema.
   */
  bundled: boolean;
}
/**
 * Um executável do bundle cujo hash não bate com o manifesto.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FileMismatch".
 */
export interface FileMismatch {
  /**
   * O componente que lista o arquivo.
   */
  component: string;
  /**
   * Caminho relativo ao bundle.
   */
  path: string;
  /**
   * O hash do manifesto.
   */
  expected: string;
  /**
   * O hash do arquivo, ou `None` se ele não existe.
   */
  actual: string | null;
}
/**
 * As versões do Lace em `lace update`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LaceVersions".
 */
export interface LaceVersions {
  /**
   * A versão deste `lace`.
   */
  installed: string;
  /**
   * A versão da última release no GitHub.
   */
  latest: string;
  /**
   * `latest` é mais nova que `installed`.
   */
  newer: boolean;
}
/**
 * As versões do bundle em `lace update`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "BundleVersions".
 */
export interface BundleVersions {
  /**
   * O bundle instalado; `null` sem bundle.
   */
  installed: string | null;
  /**
   * O bundle da última release; `null` se o `bundle/versions.json` dela
   * não pôde ser lido.
   */
  latest: string | null;
  /**
   * `latest` é mais novo que `installed`.
   */
  newer: boolean;
}
/**
 * Um componente instalado em `lace update`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ComponentVersions".
 */
export interface ComponentVersions {
  /**
   * O componente (`yanc`, `icarus`, `verilator`, `yosys`, `graphviz`,
   * `surfer-aurora`).
   */
  name: string;
  /**
   * O pacote de `bundle/versions.json` de onde ele sai (`oss-cad-suite`,
   * `yanc`, `surfer-aurora`, `graphviz`); `null` para um componente que
   * este Lace não conhece.
   */
  package: string | null;
  /**
   * A versão instalada.
   */
  installed: string;
  /**
   * A versão no bundle da última release; `null` se não pôde ser lida.
   */
  release: string | null;
  /**
   * `release` é mais nova que `installed`: chega atualizando o Lace.
   */
  release_newer: boolean;
  /**
   * A última versão upstream; `null` se a consulta falhou.
   */
  upstream: string | null;
  /**
   * `upstream` é mais nova que `installed` e que `release`: só chega num
   * bundle novo, numa release nova do Lace. Sem `release`, `false` quando
   * há Lace mais novo, cujo bundle pode já trazê-la.
   */
  upstream_newer: boolean;
}
/**
 * O que a atualização por componentes trocou.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "UpdateChanges".
 */
export interface UpdateChanges {
  /**
   * Os componentes com arquivos que entraram ou saíram, na ordem do
   * bundle; `lace` é o próprio Lace e o cabeçalho do bundle.
   */
  components: string[];
  /**
   * Bytes baixados.
   */
  download_bytes: number;
  /**
   * Arquivos que entraram (novos ou trocados).
   */
  files: number;
  /**
   * Arquivos que saíram.
   */
  removed: number;
}
/**
 * Um escopo da árvore: uma instância de módulo, ou um bloco `generate` ou
 * `begin` com nome.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SignalScope".
 */
export interface SignalScope {
  /**
   * O nome (`dut`, `g[0]`).
   */
  name: string;
  /**
   * O caminho na hierarquia (`tb.dut`), como vai na escolha.
   */
  path: string;
  /**
   * Instância de módulo, bloco `generate` ou bloco com nome.
   */
  kind: 'module' | 'generate' | 'block';
  /**
   * O módulo, numa instância.
   */
  module: string | null;
  /**
   * É um processador SAPHO (tem `valr2` e `linetabs`): os sinais dele vão
   * para a onda com qualquer escolha.
   */
  processor: boolean;
  /**
   * Os sinais, na ordem do Icarus.
   */
  signals: WaveSignal[];
  /**
   * Os escopos de dentro.
   */
  scopes: SignalScope[];
}
/**
 * Um sinal da árvore.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "WaveSignal".
 */
export interface WaveSignal {
  /**
   * O nome.
   */
  name: string;
  /**
   * O caminho na hierarquia (`tb.dut.q`).
   */
  path: string;
  /**
   * Bits (64 num `real`).
   */
  width: number;
  /**
   * `reg`, `wire`, `integer` ou `real`.
   */
  kind: 'reg' | 'wire' | 'integer' | 'real';
  /**
   * A direção, quando o sinal é uma porta do módulo.
   */
  direction: PortDirection | null;
}
/**
 * O layout salvo no projeto para a onda aberta.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SavedLayoutReport".
 */
export interface SavedLayoutReport {
  /**
   * O arquivo.
   */
  path: string;
  /**
   * O usuário salvou depois que o Lace o gerou: o Lace não o refaz
   * (`lace wave --reset-layout` volta ao gerado).
   */
  customized: boolean;
}
/**
 * Um processador SAPHO achado na onda.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "WaveProcessor".
 */
export interface WaveProcessor {
  /**
   * O escopo da instância na onda (`soma_tb.proc`).
   */
  instance: string;
  /**
   * O processador (o `#PRNAME`, nome da pasta no projeto).
   */
  processor: string;
  /**
   * Quantas variáveis do programa estão na onda (cada elemento de array
   * conta).
   */
  variables: number;
  /**
   * O PC aparece como instrução de assembly (havia `trad_opcode.txt`).
   */
  assembly: boolean;
  /**
   * A linha do fonte aparece como texto (havia `trad_cmm.txt`).
   */
  source: boolean;
  /**
   * As tabelas são mais novas que a onda: o processador foi compilado de
   * novo depois da simulação, e o assembly e a linha do C± sairiam
   * deslocados. O PC e a linha aparecem como números até simular de novo;
   * as interfaces avisam.
   */
  outdated: boolean;
}
/**
 * `lace add`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "AddReport".
 */
export interface AddReport {
  /**
   * O `.spf`.
   */
  project: string;
  /**
   * Um por arquivo, na ordem da linha de comando.
   */
  files: AddedFile[];
}
/**
 * `lace build`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "BuildReport".
 */
export interface BuildReport {
  /**
   * O `.spf`.
   */
  project: string;
  /**
   * Um por processador compilado. Vazio num projeto sem processadores.
   */
  results: BuildResult[];
  /**
   * O relatório gravado no histórico do projeto (`run-000042`), para
   * `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
   */
  report: string | null;
}
/**
 * `lace check`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "CheckReport".
 */
export interface CheckReport {
  check: CheckResult1;
  /**
   * O relatório gravado no histórico do projeto (`run-000042`), para
   * `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
   */
  report: string | null;
}
/**
 * A verificação. Não compila os processadores: verifica o Verilog que
 * está no disco.
 */
export interface CheckResult1 {
  /**
   * Os módulos elaborados como raiz: os do projeto (ou os do arquivo
   * pedido) e cada testbench.
   */
  targets: string[];
  /**
   * `Succeeded` se tudo elabora sem erro.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * O primeiro passo que falhou.
   */
  failed_step: Step | null;
  /**
   * Um `iverilog -t null` para o projeto, um por testbench e, com
   * `lint`, o `verilator --lint-only`.
   */
  steps: StepReport[];
  /**
   * Os erros e avisos, com arquivo e linha.
   */
  diagnostics: Diagnostic[];
  /**
   * Quanto a verificação levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * Qualquer comando que não conseguiu rodar (código de saída 2).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ErrorReport".
 */
export interface ErrorReport {
  error: ErrorInfo2;
}
/**
 * O erro.
 */
export interface ErrorInfo2 {
  /**
   * Código estável, em `snake_case` (`no_testbench`, `component_missing`),
   * ou `cli` para um erro da própria linha de comando.
   */
  code: string;
  /**
   * A mensagem, em inglês.
   */
  message: string;
  /**
   * O comando que resolve, quando há um.
   */
  hint?: string | null;
}
/**
 * `lace fpga boards`: as placas conhecidas, ou a pedida.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FpgaBoardsReport".
 */
export interface FpgaBoardsReport {
  /**
   * As placas, com o FPGA, o cabo e os sinais com os pinos.
   */
  boards: Board[];
}
/**
 * `lace fpga build`: os processadores, compilados antes, e a compilação
 * para a placa pelo Quartus.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FpgaBuildReport".
 */
export interface FpgaBuildReport {
  /**
   * Os processadores. Se um falha, `fpga` é `null`.
   */
  builds: BuildResult[];
  /**
   * A compilação para a placa.
   */
  fpga: FpgaBuildResult | null;
}
/**
 * `lace fpga program --list`: os cabos de gravação ligados.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FpgaCablesReport".
 */
export interface FpgaCablesReport {
  /**
   * Como o `quartus_pgm -l` os escreve (`USB-Blaster [USB-0]`); vazio,
   * nenhum.
   */
  cables: string[];
}
/**
 * `lace fpga check`: o `fpga.json` conferido.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FpgaCheckReport".
 */
export interface FpgaCheckReport {
  /**
   * O `fpga.json`.
   */
  config: string;
  resolved: Resolved1;
  /**
   * O Verilog do topo da placa que o Lace gera.
   */
  board_top: string;
  /**
   * Cada ligação como texto: o que vai em cada entrada do topo e em cada
   * sinal de saída da placa.
   */
  connections: Connection[];
}
/**
 * As ligações, bit a bit.
 */
export interface Resolved1 {
  board: Board1;
  /**
   * O módulo do topo.
   */
  top: string;
  /**
   * As portas do topo, na ordem da declaração.
   */
  ports: TopPort[];
  /**
   * As entradas do topo, com a origem de cada bit, do bit 0 em diante.
   */
  inputs: PortDrive[];
  /**
   * Os sinais de saída da placa usados, com a porta que dirige cada bit.
   */
  board_outputs: SignalDrive[];
  /**
   * Os sinais de entrada da placa usados, na ordem em que aparecem.
   */
  board_inputs: string[];
  /**
   * As portas ligadas a um clock da placa.
   */
  clocks: Clock[];
  /**
   * Os ajustes feitos: larguras completadas ou cortadas, entradas soltas.
   */
  notes: string[];
}
/**
 * O resultado de [`program`].
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "FpgaProgramResult".
 */
export interface FpgaProgramResult {
  /**
   * A placa (o `id`).
   */
  board: string;
  /**
   * O `.sof` gravado.
   */
  bitstream: string;
  /**
   * O cabo usado, como o `quartus_pgm -l` o escreve
   * (`USB-Blaster [USB-0]`).
   */
  cable: string;
  /**
   * A posição da FPGA na cadeia JTAG (o `@n`).
   */
  position: number;
  quartus: Quartus2;
  /**
   * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * `program` quando falhou.
   */
  failed_step: Step | null;
  /**
   * Um passo: `program` (`quartus_pgm`).
   */
  steps: StepReport[];
  /**
   * Os erros e avisos do Programmer.
   */
  diagnostics: Diagnostic[];
  /**
   * Quanto a gravação levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
}
/**
 * O Quartus que gravou.
 */
export interface Quartus2 {
  /**
   * A pasta `quartus` da instalação (`C:\intelFPGA_lite\22.1std\quartus`).
   */
  root: string;
  /**
   * A pasta dos programas: `bin64` no Windows, `bin` no Linux.
   */
  bin: string;
  /**
   * A versão, pelo nome da pasta da instalação (`22.1std`).
   */
  version?: string | null;
}
/**
 * O resultado de [`hierarchy`].
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "HierarchyResult".
 */
export interface HierarchyResult {
  /**
   * O design, sem testbench: os módulos que nenhum outro instancia (em
   * geral o topo), com tudo o que eles instanciam. `None` num projeto sem
   * Verilog de design.
   */
  design: Elaboration | null;
  /**
   * Cada testbench elaborado com o design: os registrados, na ordem do
   * `.spf`, e depois o testbench gerado de cada processador compilado.
   */
  testbenches: Elaboration[];
  /**
   * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * `elaborate` quando uma elaboração falhou ou foi cancelada.
   */
  failed_step: Step | null;
  /**
   * Um `iverilog` para o design e um por testbench, na ordem.
   */
  steps: StepReport[];
  /**
   * Os erros e avisos de todas as elaborações, com arquivo e linha.
   */
  diagnostics: Diagnostic[];
  /**
   * Os `.vvp` de cada elaboração.
   */
  artifacts: Artifact[];
  /**
   * Quanto as elaborações levaram juntas, em milissegundos.
   */
  duration_ms: number;
}
/**
 * `lace install`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "InstallReport".
 */
export interface InstallReport {
  /**
   * A pasta da instalação.
   */
  prefix: string;
  /**
   * Os aplicativos do bundle instalados agora.
   */
  components: string[];
  /**
   * Os que entraram nesta chamada (com os que eles exigem).
   */
  added: string[];
}
/**
 * `lace learn check`: a correção de cada exercício pedido, que também
 * grava se ele está resolvido.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LearnCheckReport".
 */
export interface LearnCheckReport {
  /**
   * Uma por exercício, na ordem da trilha.
   */
  results: Grade[];
  /**
   * Quantos exercícios da trilha estão resolvidos agora.
   */
  solved: number;
  /**
   * Quantos a trilha tem.
   */
  total: number;
}
/**
 * O resultado da conferência.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "DevReport".
 */
export interface DevReport {
  /**
   * A trilha.
   */
  track: string;
  /**
   * Cada exercício, na ordem.
   */
  exercises: DevExercise[];
}
/**
 * `lace learn hint`: as dicas de um exercício.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LearnHintReport".
 */
export interface LearnHintReport {
  /**
   * O exercício.
   */
  exercise: string;
  /**
   * O título.
   */
  title: string;
  /**
   * As dicas, em markdown, na ordem.
   */
  hints: string[];
}
/**
 * `lace learn init`: a pasta de exercícios criada.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LearnInitReport".
 */
export interface LearnInitReport {
  /**
   * A pasta de exercícios.
   */
  root: string;
  /**
   * A trilha (`verilog`).
   */
  track: string;
  /**
   * Quantos exercícios a trilha tem.
   */
  exercises: number;
  /**
   * O exercício atual (o primeiro).
   */
  current: string;
}
/**
 * `lace learn list`: os exercícios e o que já foi resolvido.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LearnListReport".
 */
export interface LearnListReport {
  /**
   * A pasta de exercícios.
   */
  root: string;
  /**
   * A trilha.
   */
  track: string;
  /**
   * O título dela.
   */
  title: string;
  /**
   * Quantos resolvidos.
   */
  solved: number;
  /**
   * Quantos exercícios.
   */
  total: number;
  /**
   * Cada exercício, na ordem.
   */
  exercises: ExerciseStatus[];
}
/**
 * `lace learn reset`: o arquivo que voltou ao começo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LearnResetReport".
 */
export interface LearnResetReport {
  /**
   * O exercício.
   */
  exercise: string;
  /**
   * O arquivo do aluno.
   */
  file: string;
}
/**
 * `lace learn wave`: a onda aberta no Surfer.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "LearnWaveReport".
 */
export interface LearnWaveReport {
  /**
   * O exercício.
   */
  exercise: string;
  /**
   * A onda da última correção.
   */
  waveform: string;
  /**
   * O arquivo de comandos com que ela abriu.
   */
  layout: string | null;
  /**
   * O processo do Surfer.
   */
  pid: number;
}
/**
 * `lace move`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "MoveReport".
 */
export interface MoveReport {
  /**
   * O `.spf`.
   */
  project: string;
  /**
   * Um por origem, na ordem da linha de comando.
   */
  moved: MovedPath[];
}
/**
 * `lace new`: o projeto criado.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "NewReport".
 */
export interface NewReport {
  /**
   * Sempre `"Project created"`.
   */
  message: string;
  /**
   * O `.spf` criado.
   */
  path: string;
  /**
   * A pasta do projeto.
   */
  root: string;
}
/**
 * `lace order`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "OrderReport".
 */
export interface OrderReport {
  /**
   * O `.spf`.
   */
  project: string;
  /**
   * A lista em que o arquivo está, na ordem nova.
   */
  files: ProjectFile[];
}
/**
 * Um processador do projeto, com todos os caminhos já absolutos.
 *
 * Os valores de simulação (`frequency_mhz`, `clocks`, `show_arrays`) vêm do
 * `.spf` (`clk`, `numClocks`, `showArrays`) ou dos padrões da AURORA
 * ([`DEFAULT_FREQUENCY_MHZ`], [`DEFAULT_CLOCKS`], `false`). Para mudá-los
 * só num build, use [`BuildOptions`](crate::BuildOptions).
 *
 * A linguagem é resolvida como na AURORA: `language` declarada no `.spf`;
 * senão a extensão de `sourceFile`/`cmmFile`; senão o arquivo que existir em
 * `Software/` (`<nome>.cmm` antes de `<nome>.cpp`); senão C±.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "Processor".
 */
export interface Processor {
  /**
   * O nome: diretório, `#PRNAME`, nome do módulo Verilog e dos artefatos.
   */
  name: string;
  /**
   * A linguagem do fonte.
   */
  language: 'cmm' | 'cpp';
  /**
   * `<raiz>/<nome>`, o `-p` dos compiladores.
   */
  dir: string;
  /**
   * O programa-fonte, em `Software/`.
   */
  source: string;
  /**
   * Diretório de intermediários, o `-t` dos compiladores.
   */
  temp_dir: string;
  /**
   * Frequência de operação em MHz (`-f` do `asmcomp`).
   */
  frequency_mhz: number;
  /**
   * Clocks a simular (`-c` do `asmcomp`, vai para o testbench).
   */
  clocks: number;
  /**
   * Exporta arrays para a simulação (`-A` do `cmmcomp`).
   */
  show_arrays: boolean;
}
/**
 * `lace remove`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "RemoveReport".
 */
export interface RemoveReport {
  /**
   * O `.spf`.
   */
  project: string;
  /**
   * Tirados do projeto (continuam no disco).
   */
  removed: string[];
  /**
   * Pedidos, mas que não estavam no projeto.
   */
  not_registered: string[];
}
/**
 * `lace report clean`: os relatórios apagados.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ReportCleanReport".
 */
export interface ReportCleanReport {
  /**
   * Os apagados, do mais antigo para o mais novo. Vazio se não havia o
   * que apagar.
   */
  removed: string[];
  /**
   * Quantos ficaram no histórico.
   */
  kept: number;
}
/**
 * Dois relatórios comparados: o atual contra a referência.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "RunComparison".
 */
export interface RunComparison {
  /**
   * O relatório atual.
   */
  current_id: string;
  current: RunMetadata2;
  /**
   * A referência.
   */
  baseline_id: string;
  baseline: RunMetadata3;
  /**
   * A síntese comparada; `null` se os dois não têm estatísticas do mesmo
   * topo.
   */
  synthesis: SynthesisComparison | null;
  /**
   * A simulação comparada; `null` se os dois não têm tempos do mesmo
   * simulador e testbench.
   */
  simulation: TimingComparison | null;
  /**
   * O que difere no contexto e pode explicar uma mudança (fontes,
   * versões, máquina), em inglês, sem ponto final.
   */
  warnings: string[];
}
/**
 * O contexto dele.
 */
export interface RunMetadata2 {
  /**
   * Quando terminou, em UTC (`2026-10-03T21:04:05Z`).
   */
  timestamp: string;
  /**
   * O comando.
   */
  command: string;
  /**
   * Como terminou.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * Quanto levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
  /**
   * A versão do Lace que gravou.
   */
  lace_version: string;
  /**
   * O que identifica o projeto: um hash da raiz dele. Relatórios de
   * projetos diferentes não se comparam.
   */
  project: string;
  /**
   * O nome do projeto.
   */
  project_name: string;
  /**
   * O bundle de ferramentas (`2026.09.29`) e a plataforma.
   */
  bundle: string;
  /**
   * `linux-x64`, `darwin-arm64`, `windows-x64`.
   */
  platform: string;
  /**
   * O módulo de topo: o da síntese, ou o testbench simulado, ou o topo do
   * projeto.
   */
  top: string | null;
  /**
   * O contexto da síntese, quando houve.
   */
  synthesis: SynthesisContext | null;
  /**
   * O contexto da simulação, quando houve.
   */
  simulation: SimulationContext | null;
  environment: Environment;
}
/**
 * O contexto dela.
 */
export interface RunMetadata3 {
  /**
   * Quando terminou, em UTC (`2026-10-03T21:04:05Z`).
   */
  timestamp: string;
  /**
   * O comando.
   */
  command: string;
  /**
   * Como terminou.
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * Quanto levou, do começo ao fim, em milissegundos.
   */
  duration_ms: number;
  /**
   * A versão do Lace que gravou.
   */
  lace_version: string;
  /**
   * O que identifica o projeto: um hash da raiz dele. Relatórios de
   * projetos diferentes não se comparam.
   */
  project: string;
  /**
   * O nome do projeto.
   */
  project_name: string;
  /**
   * O bundle de ferramentas (`2026.09.29`) e a plataforma.
   */
  bundle: string;
  /**
   * `linux-x64`, `darwin-arm64`, `windows-x64`.
   */
  platform: string;
  /**
   * O módulo de topo: o da síntese, ou o testbench simulado, ou o topo do
   * projeto.
   */
  top: string | null;
  /**
   * O contexto da síntese, quando houve.
   */
  synthesis: SynthesisContext | null;
  /**
   * O contexto da simulação, quando houve.
   */
  simulation: SimulationContext | null;
  environment: Environment;
}
/**
 * `lace report list`: os relatórios, do mais novo para o mais antigo.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ReportListReport".
 */
export interface ReportListReport {
  /**
   * Os relatórios, até o `--limit`.
   */
  reports: RunSummary[];
}
/**
 * `lace report` e `lace report show`: um relatório guardado.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ReportShowReport".
 */
export interface ReportShowReport {
  /**
   * O identificador (`run-000042`).
   */
  id: string;
  /**
   * O `report.txt`.
   */
  path: string;
  record: RunRecord1;
  /**
   * O `report.txt`, sem mudança.
   */
  text: string;
}
/**
 * O que a comparação usa: contexto, estatísticas e tempos.
 */
export interface RunRecord1 {
  /**
   * O formato ([`RECORD_SCHEMA`]).
   */
  schema: number;
  /**
   * O identificador (`run-000042`).
   */
  id: string;
  metadata: RunMetadata1;
  /**
   * As estatísticas do Yosys, quando houve síntese e o Yosys as gravou.
   */
  synthesis: SynthesisStatistics | null;
  /**
   * Os tempos da simulação, quando houve simulação.
   */
  simulation: SimulationTimings | null;
}
/**
 * `lace sim`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SimReport".
 */
export interface SimReport {
  /**
   * Os processadores, compilados antes. Se um falha, `simulation` é
   * `null`.
   */
  builds: BuildResult[];
  /**
   * A simulação.
   */
  simulation: SimulationResult | null;
  /**
   * Com `-p`, o que a simulação escreveu em cada porta de saída.
   */
  outputs: PortValues[];
  /**
   * Com `--open`, o processo do surfer-aurora.
   */
  surfer_pid: number | null;
  /**
   * O relatório gravado no histórico do projeto (`run-000042`), para
   * `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
   */
  report: string | null;
}
/**
 * `lace status`: o projeto inteiro.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "StatusReport".
 */
export interface StatusReport {
  /**
   * Nome do projeto (o do `.spf`).
   */
  name: string;
  /**
   * O `.spf`.
   */
  spf: string;
  /**
   * A pasta do projeto.
   */
  root: string;
  /**
   * Os módulos registrados.
   */
  synthesizable: ProjectFile[];
  /**
   * Os testbenches registrados.
   */
  testbench: ProjectFile[];
  /**
   * O arquivo de topo, se escolhido.
   */
  top_level: string | null;
  /**
   * O módulo de topo: o `topLevelModule` do `.spf`, o único módulo do
   * arquivo de topo ou o que tem o nome dele. `null` se não dá para saber.
   */
  top_module: string | null;
  /**
   * O testbench que `lace sim` simula.
   */
  selected_testbench: string | null;
  /**
   * O módulo desse testbench.
   */
  testbench_module: string | null;
  /**
   * Arquivos `.v` e `.sv` da pasta que não estão no projeto.
   */
  unregistered: string[];
  /**
   * O processador em cuja pasta o comando rodou: o que `build`, `check`,
   * `sim`, `wave`, `synth` e `proc set` usam quando não recebem nome.
   * `null` fora da pasta de um processador.
   */
  here: string | null;
  /**
   * Os processadores SAPHO.
   */
  processors: ProcessorStatus[];
  /**
   * O que está estranho no `.spf` sem impedir de abrir
   * (`Project::issues`): caminho de outra máquina achado pela cauda,
   * topo fora da lista, processador com nome que não compila.
   */
  issues: ProjectIssue[];
}
/**
 * `lace synth`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "SynthReport".
 */
export interface SynthReport {
  /**
   * Os processadores, compilados antes. Se um falha, o resto é `null`.
   */
  builds: BuildResult[];
  /**
   * A síntese.
   */
  synthesis: SynthesisResult | null;
  /**
   * Com `--svg`, o esquemático.
   */
  schematic: SchematicResult | null;
  /**
   * O relatório gravado no histórico do projeto (`run-000042`), para
   * `lace report show`; `null` se nada rodou ou se não pôde ser gravado.
   */
  report: string | null;
}
/**
 * `lace tools`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "ToolsReport".
 */
export interface ToolsReport {
  /**
   * A pasta do bundle.
   */
  root: string;
  /**
   * A versão do bundle.
   */
  bundle: string;
  /**
   * A plataforma do bundle (`linux-x64`, `darwin-arm64`, `windows-x64`).
   */
  platform: string;
  /**
   * Os componentes instalados.
   */
  components: BundleComponent[];
  /**
   * Os componentes que este Lace conhece e que não estão instalados.
   */
  not_installed: string[];
  /**
   * Cada ferramenta, pelo nome do executável.
   */
  tools: {
    [k: string]: ToolEntry | undefined;
  };
  /**
   * O compilador do Verilator, se encontrado: do sistema no Linux e no
   * macOS, do bundle no Windows (`bundled`).
   */
  system_compiler: SystemCompiler | null;
  /**
   * O Quartus Prime do sistema, que compila para as placas Intel, se
   * encontrado.
   */
  quartus: Quartus1 | null;
  /**
   * Com `--verify`, os executáveis cujo SHA-256 não confere (vazio: todos
   * conferem).
   */
  verify: FileMismatch[] | null;
}
/**
 * `lace top`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "TopReport".
 */
export interface TopReport {
  /**
   * O arquivo de topo, se escolhido.
   */
  top_level: string | null;
  /**
   * O módulo de topo.
   */
  module: string | null;
  /**
   * Por que não dá para saber o módulo de um arquivo de topo escolhido
   * (vários módulos, nenhum com o nome do arquivo).
   */
  module_error: ErrorInfo | null;
}
/**
 * `lace uninstall`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "UninstallReport".
 */
export interface UninstallReport {
  /**
   * A pasta da instalação.
   */
  prefix: string;
  /**
   * `true`: a pasta e o atalho já saíram (Linux, macOS). `false`: o
   * desinstalador do Windows foi aberto e termina depois que o `lace` sai.
   */
  removed: boolean;
}
/**
 * `lace update`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "UpdateReport".
 */
export interface UpdateReport {
  lace: LaceVersions1;
  bundle: BundleVersions1;
  /**
   * Cada componente instalado, na ordem do manifesto do bundle. Vazio sem
   * bundle.
   */
  components: ComponentVersions[];
  /**
   * A pasta da instalação; `null` se este `lace` não foi instalado pelo
   * instalador (um build em `target/`, por exemplo).
   */
  prefix: string | null;
  /**
   * O que o comando fez.
   */
  action: 'checked' | 'up_to_date' | 'updated' | 'wizard_opened';
  /**
   * Como atualizou; `null` quando não atualizou.
   */
  method: UpdateMethod | null;
  /**
   * O que a atualização por componentes trocou, ou vai trocar com
   * `--check --from`; `null` quando ela não rodou.
   */
  changes: UpdateChanges | null;
}
/**
 * O Lace: este e o da última release.
 */
export interface LaceVersions1 {
  /**
   * A versão deste `lace`.
   */
  installed: string;
  /**
   * A versão da última release no GitHub.
   */
  latest: string;
  /**
   * `latest` é mais nova que `installed`.
   */
  newer: boolean;
}
/**
 * O bundle de ferramentas: o instalado e o da última release.
 */
export interface BundleVersions1 {
  /**
   * O bundle instalado; `null` sem bundle.
   */
  installed: string | null;
  /**
   * O bundle da última release; `null` se o `bundle/versions.json` dela
   * não pôde ser lido.
   */
  latest: string | null;
  /**
   * `latest` é mais novo que `installed`.
   */
  newer: boolean;
}
/**
 * `lace wave select` e `lace wave unselect`: a escolha de sinais depois da
 * mudança.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "WaveChoiceReport".
 */
export interface WaveChoiceReport {
  /**
   * O módulo do testbench do projeto.
   */
  testbench: string;
  /**
   * O arquivo da escolha (`wave/<testbench>.json`); `null` quando ela
   * ficou vazia e o arquivo saiu: a onda grava todos os sinais.
   */
  file: string | null;
  /**
   * A escolha; vazia: todos os sinais.
   */
  selection: string[];
}
/**
 * A árvore de sinais do testbench do projeto, para escolher o que a onda
 * grava ([`wave_signals`]).
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "WaveSignals".
 */
export interface WaveSignals {
  /**
   * O testbench elaborado (o da simulação do projeto).
   */
  testbench: string;
  /**
   * O módulo do testbench: a raiz da árvore e o nome dos arquivos em
   * `wave/`.
   */
  module: string;
  /**
   * A árvore, a partir do módulo do testbench. `None` quando a elaboração
   * falhou (os erros em `diagnostics`).
   */
  root: SignalScope | null;
  /**
   * A escolha gravada; vazia: a simulação grava todos os sinais.
   */
  selection: string[];
  /**
   * Onde a escolha fica (`wave/<módulo>.json`), exista ou não.
   */
  selection_file: string;
  /**
   * Itens da escolha que a árvore não tem (um sinal renomeado, um módulo
   * que saiu): a simulação os deixa de fora.
   */
  unknown: string[];
  /**
   * Como uma operação terminou. Em JSON, em `snake_case` (`"succeeded"`).
   */
  status: 'succeeded' | 'failed' | 'crashed' | 'incomplete' | 'cancelled' | 'timed_out';
  /**
   * A elaboração (`iverilog`).
   */
  steps: StepReport[];
  /**
   * Os erros e avisos da elaboração.
   */
  diagnostics: Diagnostic[];
  /**
   * Quanto levou, em milissegundos.
   */
  duration_ms: number;
}
/**
 * `lace wave`.
 *
 * This interface was referenced by `LaceSchemas`'s JSON-Schema
 * via the `definition` "WaveReport".
 */
export interface WaveReport {
  /**
   * A onda aberta.
   */
  waveform: string;
  /**
   * O processo do surfer-aurora, que continua aberto.
   */
  pid: number;
  /**
   * Onde vai o stdout e o stderr do surfer-aurora.
   */
  log: string;
  /**
   * O estado do Surfer com que a onda abriu (`.surf.ron`): o layout salvo
   * no projeto ou o gerado; `null` com `--no-layout`, numa onda sem sinal
   * para mostrar ou que não é VCD nem FST.
   */
  layout: string | null;
  /**
   * O layout salvo no projeto (`wave/<testbench>.surf.ron`), onde o
   * Surfer salva (Ctrl+S); `null` numa onda fora de projeto.
   */
  saved_layout: SavedLayoutReport | null;
  /**
   * Os processadores do layout.
   */
  processors: WaveProcessor[];
}
