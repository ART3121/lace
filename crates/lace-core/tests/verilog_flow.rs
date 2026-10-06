//! O fluxo Verilog: classificar arquivos, registrar, escolher topo e
//! testbench, verificar, simular e achar a onda, com ou sem processadores
//! SAPHO.
//!
//! Os testes da primeira metade não executam ferramenta nenhuma e rodam
//! sempre. Os da segunda executam Icarus, Yosys ou Verilator do bundle e
//! precisam de `LACE_TEST_BUNDLE` (ver `common/mod.rs`).

mod common;

use camino::{Utf8Path, Utf8PathBuf};
use lace_core::verilog::{
    ModuleInterface, PortDirection, classify, module_template, modules_in, read_interfaces,
    testbench_template,
};
use lace_core::{
    ArtifactKind, BuildOptions, CheckOptions, CheckResult, Control, DesignTarget, FileRole,
    HierarchyOptions, LaceError, Language, ModuleInstance, NewProcessor, Project, Severity,
    SimulationOptions, Simulator, Status, Step, StepReport, Tool, WaveformFormat, build, check,
    hierarchy, simulate, simulate_project, synthesize, waveform_path,
};

use PortDirection::{Inout, Input, Output};

// ------------------------------------------------------------ fontes

/// Porta E com um sinal interno.
const AND_GATE: &str = "\
module and_gate (
    input  wire a,
    input  wire b,
    output wire y
);
    wire t;
    assign t = a & b;
    assign y = t;
endmodule
";

/// Ninguém instancia; o erro (linha 2) só aparece se ele for elaborado.
const ORPHAN: &str = "\
module orphan (input wire a, output wire y);
    assign y = a & bogus;
endmodule
";

/// Instancia `and_gate`, que vem de outro arquivo.
const MUX2: &str = "\
module mux2 (
    input  wire a,
    input  wire b,
    input  wire sel,
    output wire y
);
    wire t0, t1;
    and_gate u0 (.a(a), .b(~sel), .y(t0));
    and_gate u1 (.a(b), .b(sel), .y(t1));
    assign y = t0 | t1;
endmodule
";

/// Testbench com erro de sintaxe na linha 6 (falta `;`).
const BROKEN_TB: &str = "\
module and_gate_tb;
    reg a, b;
    wire y;
    and_gate dut (.a(a), .b(b), .y(y));
    initial begin
        a = 0 b = 1;
        #10 $finish;
    end
endmodule
";

/// O vvp sai com 0 depois de um `$error`; a simulação tem de falhar mesmo
/// assim.
const FAILING_TB: &str = "\
module and_gate_tb;
    reg a = 0, b = 1;
    wire y;
    and_gate dut (.a(a), .b(b), .y(y));
    initial begin
        #1;
        if (y !== 1'b1) $error(\"y deveria ser 1, veio %b\", y);
        $finish;
    end
endmodule
";

/// Testbench sem `$dumpfile` num arquivo que não tem o nome do módulo.
const BANCADA: &str = "\
// Estímulo da porta E.
module bancada;
    reg a = 1, b = 1;
    wire y;
    and_gate dut (.a(a), .b(b), .y(y));
    initial begin
        #1 $display(\"y = %b\", y);
        $finish;
    end
endmodule
";

/// Atribui 8 bits a 4: o Icarus aceita calado, o Verilator avisa (linha 5).
const WIDTHY: &str = "\
module widthy (
    input  wire [7:0] a,
    output wire [3:0] y
);
    assign y = a;
endmodule
";

/// Um módulo do usuário com o nome do módulo principal da biblioteca SAPHO.
const USER_PROCESSOR: &str = "\
module processor (
    input  wire [3:0] a,
    output wire [3:0] y
);
    assign y = ~a;
endmodule
";

const USER_PROCESSOR_TB: &str = "\
module processor_tb;
    reg [3:0] a = 4'd5;
    wire [3:0] y;
    processor dut (.a(a), .y(y));
    initial begin
        #1 $display(\"y = %0d\", y);
        $finish;
    end
endmodule
";

/// Portas ANSI com larguras literais: o que o leitor embutido precisa ler
/// sem o Yosys.
const ALU: &str = "\
module alu (
    input               clk,
    input  wire         rst,
    input  wire [7:0]   a,      // operando
    input  signed [3:0] s,
    output reg  [7:0]   y,
    output wire         v,
    inout  wire [0:3]   bus
);
    always @(posedge clk) y <= rst ? 8'd0 : a + s;
    assign v = |y;
    assign bus = 4'bz;
endmodule
";

/// ANSI com parâmetros, `signed`, `inout` e faixa crescente.
const ANSI_DUT: &str = "\
module ansi_dut #(
    parameter WIDTH = 8,
    parameter [3:0] MODE = 4'd2
) (
    input  wire              clk,
    input  wire              rst_n,
    input  wire [WIDTH-1:0]  din,
    input  wire signed [7:0] sdin,
    output reg  [7:0]        dout,
    output wire              valid,
    inout  wire [3:0]        bus,
    input  wire [0:3]        little
);
    always @(posedge clk or negedge rst_n)
        if (!rst_n) dout <= 8'd0;
        else        dout <= din[7:0] + sdin;
    assign valid = |dout;
    assign bus = valid ? little : 4'bz;
endmodule
";

/// Dois módulos não ANSI; no `ord` a ordem do cabeçalho não é a das
/// declarações.
const NON_ANSI: &str = "\
module nonansi_dut (clk, a, b, q, io);
    parameter N = 4;
    input clk;
    input [N-1:0] a;
    input [7:0] b;
    output [N:0] q;
    inout io;
    reg [N:0] q;
    always @(posedge clk) q <= a + b[N-1:0];
endmodule

module ord (zz, bb, aa);
    input [8:1] aa;
    input signed [3:0] bb;
    output zz;
    assign zz = ^aa ^ ^bb;
endmodule
";

const SV_PORTS: &str = "\
module sv_ports (
    input  logic       clk,
    input  logic [7:0] a,
    output logic [7:0] y
);
    always_ff @(posedge clk) y <= a;
endmodule
";

// ------------------------------------------------------------ apoio

fn project() -> (tempfile::TempDir, Project) {
    let (guard, dir) = common::tempdir();
    let project = Project::create(&dir, "p").unwrap();
    (guard, project)
}

/// Grava um arquivo (e as pastas) e devolve o caminho.
fn write(path: impl AsRef<Utf8Path>, text: &str) -> Utf8PathBuf {
    let path = path.as_ref().to_owned();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn read(path: &Utf8Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

fn check_options(file: Option<&Utf8Path>, lint: bool) -> CheckOptions {
    let mut options = CheckOptions::default();
    options.file = file.map(Utf8Path::to_owned);
    options.lint = lint;
    options
}

fn ports(interface: &ModuleInterface) -> Vec<(&str, PortDirection, u32, bool)> {
    interface
        .ports
        .iter()
        .map(|p| (p.name.as_str(), p.direction, p.width, p.signed))
        .collect()
}

fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("//") {
            rest = after.find('\n').map_or("", |at| &after[at..]);
        } else if let Some(after) = rest.strip_prefix("/*") {
            rest = after.find("*/").map_or("", |at| &after[at + 2..]);
            out.push(' ');
        } else {
            out.push(c);
            rest = &rest[c.len_utf8()..];
        }
    }
    out
}

/// Identificadores e pontuação, fora de comentários.
fn tokens(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut word = String::new();
    for c in strip_comments(text).chars() {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            out.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() {
            out.push(c.to_string());
        }
    }
    if !word.is_empty() {
        out.push(word);
    }
    out
}

fn is_identifier(token: &str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

/// O texto instancia `module` (`module nome (` ou `module #(`)?
fn instantiates(text: &str, module: &str) -> bool {
    tokens(text)
        .windows(3)
        .any(|w| w[0] == module && (w[1] == "#" || (is_identifier(&w[1]) && w[2] == "(")))
}

/// A declaração de `name` (`reg`, `wire` ou `logic`) e o texto dela, para
/// conferir largura e sinal.
fn declaration(text: &str, name: &str) -> Option<(String, String)> {
    for statement in strip_comments(text).split(';') {
        let statement = statement.trim();
        let Some(kind) = ["reg", "wire", "logic"]
            .into_iter()
            .find(|k| statement.split_whitespace().next() == Some(k))
        else {
            continue;
        };
        // O nome de cada item é o último identificador antes do `=`.
        let declared = statement[kind.len()..].split(',').any(|item| {
            let item = item.split('=').next().unwrap_or_default();
            item.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .rfind(|w| !w.is_empty())
                == Some(name)
        });
        if declared {
            return Some((kind.to_owned(), statement.to_owned()));
        }
    }
    None
}

/// Sem espaços, para procurar `.a(` e `clk=~clk` sem depender da formatação.
fn compact(text: &str) -> String {
    strip_comments(text)
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect()
}

/// Os módulos passados com `-s` num passo do `iverilog`.
fn roots(step: &StepReport) -> Vec<&str> {
    let args = &step.command.args;
    args.iter()
        .zip(args.iter().skip(1))
        .filter(|(flag, _)| *flag == "-s")
        .map(|(_, module)| module.as_str())
        .collect()
}

fn steps(result: &CheckResult, step: Step) -> Vec<&StepReport> {
    result.steps.iter().filter(|s| s.step == step).collect()
}

/// Algum passo recebeu a biblioteca SAPHO (`-y <HDL>`)?
fn uses_sapho_library(steps: &[StepReport]) -> bool {
    steps
        .iter()
        .any(|s| s.command.args.iter().any(|a| a == "-y"))
}

fn has_arg(step: &StepReport, arg: &str) -> bool {
    step.command.args.iter().any(|a| a == arg)
}

// ------------------------------------------------------------ classify

#[test]
fn plain_module_is_synthesizable() {
    assert_eq!(classify(AND_GATE, "and_gate.v"), FileRole::Synthesizable);
    assert_eq!(classify(MUX2, "mux2.v"), FileRole::Synthesizable);
    // Vazio (um header, por exemplo): na dúvida, sintetizável.
    assert_eq!(classify("", "defs.vh"), FileRole::Synthesizable);
}

#[test]
fn dump_and_finish_make_a_testbench() {
    // `$dumpvars` (+3) sozinho já passa do limiar, e `$finish` (+3) também.
    let dump = "module m (input wire a);\n  initial $dumpvars(0, m);\nendmodule\n";
    let finish = "module m (input wire a);\n  always @(a) if (a) $finish;\nendmodule\n";
    assert_eq!(classify(dump, "m.v"), FileRole::Testbench);
    assert_eq!(classify(finish, "m.v"), FileRole::Testbench);
    assert_eq!(classify(FAILING_TB, "and_gate_tb.v"), FileRole::Testbench);
    assert_eq!(classify(BANCADA, "estimulo.v"), FileRole::Testbench);
}

#[test]
fn portless_module_is_a_testbench() {
    for text in [
        "module bancada;\nendmodule\n",
        "module bancada ();\nendmodule\n",
        "module bancada #(parameter N = 2) ();\nendmodule\n",
    ] {
        assert_eq!(classify(text, "bancada.v"), FileRole::Testbench, "{text}");
    }
}

#[test]
fn file_name_hint_needs_company() {
    // Só o nome (+2) não basta; com um `$display` (+1), chega a 3.
    let quiet = "\
module contador (input wire clk, output reg q);
    always @(posedge clk) q <= ~q;
endmodule
";
    let chatty = "\
module contador (input wire clk, output reg q);
    always @(posedge clk) begin
        q <= ~q;
        $display(\"q\");
    end
endmodule
";
    assert_eq!(classify(quiet, "contador_tb.v"), FileRole::Synthesizable);
    assert_eq!(classify(chatty, "contador.v"), FileRole::Synthesizable);
    assert_eq!(classify(chatty, "contador_tb.v"), FileRole::Testbench);
    assert_eq!(classify(chatty, "tb_contador.v"), FileRole::Testbench);
    assert_eq!(classify(chatty, "test_contador.v"), FileRole::Testbench);
    // `tb` dentro de outra palavra não é dica.
    assert_eq!(classify(chatty, "stbox.v"), FileRole::Synthesizable);
}

#[test]
fn rom_initialization_stays_synthesizable() {
    // `initial` (+2) para carregar uma ROM não faz do módulo um testbench.
    let rom = "\
module rom (input wire [3:0] addr, output wire [7:0] data);
    reg [7:0] mem [0:15];
    initial $readmemh(\"rom.hex\", mem);
    assign data = mem[addr];
endmodule
";
    assert_eq!(classify(rom, "rom.v"), FileRole::Synthesizable);
    // `#(8)` é parâmetro de instância, não atraso.
    let with_parameter = "\
module banco (input wire [7:0] d, output wire [7:0] q);
    reg [7:0] init;
    initial init = 8'd0;
    registro #(8) r0 (.d(d), .q(q));
endmodule
";
    assert_eq!(classify(with_parameter, "banco.v"), FileRole::Synthesizable);
    // Já `initial` com atraso (+2 +1) chega ao limiar.
    let delayed = "\
module pulso (input wire a, output reg y);
    initial begin
        #5 y = a;
    end
endmodule
";
    assert_eq!(classify(delayed, "pulso.v"), FileRole::Testbench);
}

#[test]
fn classify_ignores_comments() {
    let text = "\
// $finish e $dumpvars só aqui no comentário
module and_gate (input wire a, input wire b, output wire y);
    /* initial begin #10 $finish; end */
    assign y = a & b;
endmodule
";
    assert_eq!(classify(text, "and_gate.v"), FileRole::Synthesizable);
}

// ------------------------------------------------------------ modules_in

#[test]
fn modules_in_skips_comments() {
    assert_eq!(
        modules_in("module a (input x); endmodule // module b (input y);"),
        ["a"]
    );
    let text = "\
/* module escondido;
endmodule */
module visivel (input wire a, output wire y);
    assign y = a; // module outro;
endmodule
";
    assert_eq!(modules_in(text), ["visivel"]);
    assert!(modules_in("// só comentário\n").is_empty());
}

#[test]
fn modules_in_finds_every_module_in_order() {
    let text = format!(
        "{AND_GATE}\n{ORPHAN}\nmodule\n    quebrado_em_linhas\n    (input wire a);\nendmodule\n"
    );
    assert_eq!(
        modules_in(&text),
        ["and_gate", "orphan", "quebrado_em_linhas"]
    );
    assert_eq!(modules_in(NON_ANSI), ["nonansi_dut", "ord"]);
}

#[test]
fn modules_in_with_parameter_block() {
    let text = "\
module fifo #(parameter DEPTH = 16, parameter W = 8) (
    input wire clk
);
endmodule
";
    assert_eq!(modules_in(text), ["fifo"]);
    assert_eq!(modules_in(ANSI_DUT), ["ansi_dut"]);
}

// ------------------------------------------------------------ modelos

#[test]
fn module_template_is_a_synthesizable_module() {
    let text = module_template("contador");
    assert_eq!(classify(&text, "contador.v"), FileRole::Synthesizable);
    assert_eq!(modules_in(&text), ["contador"]);

    // As portas de exemplo do SPEC: `input wire [7:0] a`, `output wire [7:0] y`.
    let (_guard, dir) = common::tempdir();
    let file = write(dir.join("contador.v"), &text);
    let interfaces = read_interfaces(None, std::slice::from_ref(&file)).unwrap();
    assert_eq!(interfaces.len(), 1);
    assert_eq!(interfaces[0].name, "contador");
    assert_eq!(
        ports(&interfaces[0]),
        [("a", Input, 8, false), ("y", Output, 8, false)]
    );
}

#[test]
fn testbench_template_without_dut() {
    let text = testbench_template("vazio_tb", None);
    assert_eq!(classify(&text, "vazio_tb.v"), FileRole::Testbench);
    assert_eq!(modules_in(&text), ["vazio_tb"]);
    assert!(text.contains("$finish"), "{text}");
}

#[test]
fn embedded_reader_reads_ansi_ports() {
    let (_guard, dir) = common::tempdir();
    let file = write(dir.join("alu.v"), ALU);
    let interfaces = read_interfaces(None, std::slice::from_ref(&file)).unwrap();
    assert_eq!(interfaces.len(), 1);
    let alu = &interfaces[0];
    assert_eq!(alu.name, "alu");
    assert_eq!(alu.file, file);
    assert_eq!(
        ports(alu),
        [
            ("clk", Input, 1, false),
            ("rst", Input, 1, false),
            ("a", Input, 8, false),
            ("s", Input, 4, true),
            ("y", Output, 8, false),
            ("v", Output, 1, false),
            ("bus", Inout, 4, false),
        ]
    );
}

#[test]
fn testbench_template_drives_every_port() {
    let (_guard, dir) = common::tempdir();
    let file = write(dir.join("alu.v"), ALU);
    let alu = read_interfaces(None, &[file]).unwrap().remove(0);
    let text = testbench_template("alu_tb", Some(&alu));

    assert_eq!(classify(&text, "alu_tb.v"), FileRole::Testbench);
    assert_eq!(modules_in(&text), ["alu_tb"], "{text}");
    assert!(instantiates(&text, "alu"), "{text}");

    // Entradas são `reg` (o testbench as dirige); saídas e `inout`, `wire`.
    for (name, kind, range) in [
        ("clk", "reg", None),
        ("rst", "reg", None),
        ("a", "reg", Some("[7:0]")),
        ("s", "reg", Some("[3:0]")),
        ("y", "wire", Some("[7:0]")),
        ("v", "wire", None),
        ("bus", "wire", Some("[3:0]")),
    ] {
        let (declared, statement) =
            declaration(&text, name).unwrap_or_else(|| panic!("{name} não declarado:\n{text}"));
        assert_eq!(declared, kind, "{name}: {statement}");
        if let Some(range) = range {
            assert!(statement.contains(range), "{name}: {statement}");
        }
    }
    let (_, s) = declaration(&text, "s").unwrap();
    assert!(s.contains("signed"), "{s}");

    let code = compact(&text);
    for port in ["clk", "rst", "a", "s", "y", "v", "bus"] {
        assert!(code.contains(&format!(".{port}(")), "porta {port}:\n{text}");
    }
    // Há `clk`: um gerador de clock. Há `rst`: o reset é dirigido.
    assert!(
        code.contains("clk=~clk") || code.contains("clk=!clk"),
        "{text}"
    );
    assert!(code.contains("rst=") || code.contains("rst<="), "{text}");
    // Onda com todos os sinais, inclusive os do módulo testado, e fim.
    assert!(code.contains("$dumpfile("), "{text}");
    assert!(code.contains("$dumpvars(0,alu_tb)"), "{text}");
    assert!(code.contains("$finish"), "{text}");
}

// ------------------------------------------------------------ projeto

#[test]
fn add_verilog_creates_module_and_makes_it_top() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let added = project.add_verilog(None, "rtl/and_gate.v", false).unwrap();
    let path = root.join("rtl/and_gate.v");
    assert_eq!(added.path, path);
    assert_eq!(added.role, FileRole::Synthesizable);
    assert!(added.created);
    assert!(added.selected, "o primeiro sintetizável vira o topo");

    let text = read(&path);
    assert_eq!(classify(&text, "and_gate.v"), FileRole::Synthesizable);
    assert_eq!(modules_in(&text), ["and_gate"]);
    assert_eq!(project.top_level(), Some(path.clone()));
    assert_eq!(project.top_module().unwrap().as_deref(), Some("and_gate"));
    assert_eq!(project.testbench_module().unwrap(), None);

    // Registrar de novo não recria nem duplica.
    std::fs::write(&path, AND_GATE).unwrap();
    let again = project.add_verilog(None, &path, false).unwrap();
    assert!(!again.created);
    assert_eq!(read(&path), AND_GATE, "o Lace não sobrescreve");

    let reopened = Project::open(project.spf_path()).unwrap();
    assert_eq!(reopened.files(FileRole::Synthesizable).len(), 1);
    assert!(reopened.files(FileRole::Testbench).is_empty());
    assert_eq!(reopened.top_level(), Some(path));
}

#[test]
fn new_testbench_tests_the_module_of_its_name() {
    let (_guard, mut project) = project();
    project.add_verilog(None, "rtl/alu.v", false).unwrap();
    let mux = project.add_verilog(None, "rtl/mux.v", false).unwrap();
    assert!(!mux.selected, "o topo continua o alu");

    let tb = project.add_verilog(None, "rtl/mux_tb.v", false).unwrap();
    assert_eq!(tb.role, FileRole::Testbench);
    assert!(tb.created);
    assert!(tb.selected, "o primeiro testbench é o escolhido");
    let text = read(&tb.path);
    assert_eq!(classify(&text, "mux_tb.v"), FileRole::Testbench);
    assert_eq!(modules_in(&text), ["mux_tb"]);
    assert!(instantiates(&text, "mux"), "{text}");
    assert!(!instantiates(&text, "alu"), "{text}");
    // As portas do módulo-modelo, lidas sem o Yosys.
    let code = compact(&text);
    assert!(code.contains(".a(") && code.contains(".y("), "{text}");

    let other = project.add_verilog(None, "rtl/tb_mux.v", false).unwrap();
    assert_eq!(other.role, FileRole::Testbench);
    assert!(!other.selected);
    assert!(instantiates(&read(&other.path), "mux"));

    assert_eq!(project.testbench(), Some(tb.path));
    assert_eq!(
        project.testbench_module().unwrap().as_deref(),
        Some("mux_tb")
    );
}

#[test]
fn new_testbench_falls_back_to_top_then_to_nothing() {
    let (_guard, mut project) = project();
    // Sem sintetizável nenhum: um testbench que só termina.
    let alone = project.add_verilog(None, "sozinho_tb.v", false).unwrap();
    assert_eq!(alone.role, FileRole::Testbench);
    let text = read(&alone.path);
    assert_eq!(classify(&text, "sozinho_tb.v"), FileRole::Testbench);
    assert_eq!(modules_in(&text), ["sozinho_tb"]);
    assert!(text.contains("$finish"));

    // Sem módulo com o nome do testbench: o de topo.
    project.add_verilog(None, "alu.v", false).unwrap();
    let general = project.add_verilog(None, "geral_tb.v", false).unwrap();
    assert!(instantiates(&read(&general.path), "alu"));
}

#[test]
fn new_file_role_comes_from_name_or_flag() {
    let (_guard, mut project) = project();
    for (name, flag, role) in [
        ("alu.v", false, FileRole::Synthesizable),
        ("tb_alu.v", false, FileRole::Testbench),
        ("alu_test.v", false, FileRole::Testbench),
        ("estimulo.v", true, FileRole::Testbench),
    ] {
        let added = project.add_verilog(None, name, flag).unwrap();
        assert_eq!(added.role, role, "{name}");
        assert!(added.created, "{name}");
        let text = read(&added.path);
        assert_eq!(classify(&text, name), role, "{name}:\n{text}");
        // O módulo do arquivo novo tem o nome do arquivo.
        let stem = name.trim_end_matches(".v");
        assert_eq!(modules_in(&text), [stem], "{name}");
    }
}

#[test]
fn existing_file_role_comes_from_contents() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let module = write(root.join("rtl/and_gate.v"), AND_GATE);
    let bench = write(root.join("rtl/estimulo.v"), BANCADA);
    let forced = write(root.join("rtl/mux2.v"), MUX2);

    let added = project.add_verilog(None, &module, false).unwrap();
    assert_eq!(
        (added.role, added.created, added.selected),
        (FileRole::Synthesizable, false, true)
    );
    let added = project.add_verilog(None, &bench, false).unwrap();
    assert_eq!(
        (added.role, added.created, added.selected),
        (FileRole::Testbench, false, true)
    );
    // `testbench` força o papel, qualquer que seja o conteúdo.
    let added = project.add_verilog(None, &forced, true).unwrap();
    assert_eq!(added.role, FileRole::Testbench);

    assert_eq!(read(&module), AND_GATE);
    assert_eq!(read(&bench), BANCADA);
    assert_eq!(read(&forced), MUX2);
}

#[test]
fn a_file_has_a_single_role() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let gate = write(root.join("and_gate.v"), AND_GATE);
    let in_list = |project: &Project, role| {
        project
            .files(role)
            .iter()
            .filter(|f| f.path == gate)
            .count()
    };

    project.add_verilog(None, &gate, false).unwrap();
    assert_eq!(project.top_level(), Some(gate.clone()));

    // Vira testbench: sai dos sintetizáveis e deixa de ser o topo.
    let moved = project.add_verilog(None, &gate, true).unwrap();
    assert_eq!(moved.role, FileRole::Testbench);
    assert_eq!(in_list(&project, FileRole::Synthesizable), 0);
    assert_eq!(in_list(&project, FileRole::Testbench), 1);
    assert_eq!(project.top_level(), None);

    // E volta, pelo conteúdo.
    let back = project.add_verilog(None, &gate, false).unwrap();
    assert_eq!(back.role, FileRole::Synthesizable);
    assert_eq!(in_list(&project, FileRole::Synthesizable), 1);
    assert_eq!(in_list(&project, FileRole::Testbench), 0);
    assert_eq!(project.testbench(), None);

    // Um módulo registrado como testbench por engano: escolhê-lo como topo
    // também o tira dos testbenches.
    let mux = write(root.join("mux2.v"), MUX2);
    project.add_verilog(None, &mux, true).unwrap();
    project.set_top(mux.as_str()).unwrap();
    let reopened = Project::open(project.spf_path()).unwrap();
    assert!(
        reopened
            .files(FileRole::Testbench)
            .iter()
            .all(|f| f.path != mux)
    );
    assert_eq!(reopened.top_level(), Some(mux));
}

#[test]
fn paths_are_normalized() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let gate = root.join("and_gate.v");

    let added = project
        .add_verilog(None, "rtl/../and_gate.v", false)
        .unwrap();
    assert_eq!(added.path, gate);
    assert!(gate.is_file());

    // `sub/` nem existe: a normalização é léxica.
    let again = project
        .add_verilog(None, root.join("sub/../and_gate.v"), false)
        .unwrap();
    assert_eq!(again.path, gate);
    assert!(!again.created);
    assert_eq!(project.files(FileRole::Synthesizable).len(), 1);
    assert_eq!(project.top_level(), Some(gate.clone()));

    // O .spf guarda o caminho limpo, relativo à raiz, como a AURORA.
    let doc: serde_json::Value = serde_json::from_str(&read(project.spf_path())).unwrap();
    assert_eq!(
        doc["structure"]["synthesizableFiles"][0]["path"],
        "and_gate.v"
    );
    assert_eq!(doc["structure"]["topLevelFile"], "and_gate.v");

    assert!(project.remove_verilog("x/../and_gate.v").unwrap());
    assert!(project.files(FileRole::Synthesizable).is_empty());
}

#[test]
fn first_of_each_role_is_selected() {
    let (_guard, mut project) = project();
    let alu = project.add_verilog(None, "alu.v", false).unwrap();
    let mux = project.add_verilog(None, "mux.v", false).unwrap();
    let alu_tb = project.add_verilog(None, "alu_tb.v", false).unwrap();
    let mux_tb = project.add_verilog(None, "mux_tb.v", false).unwrap();
    assert_eq!(
        [alu.selected, mux.selected, alu_tb.selected, mux_tb.selected],
        [true, false, true, false]
    );
    assert_eq!(project.top_level(), Some(alu.path));
    assert_eq!(project.testbench(), Some(alu_tb.path));
    assert_eq!(project.top_module().unwrap().as_deref(), Some("alu"));
    assert_eq!(
        project.testbench_module().unwrap().as_deref(),
        Some("alu_tb")
    );
}

#[test]
fn remove_verilog_unregisters_and_clears_selection() {
    let (_guard, mut project) = project();
    let alu = project.add_verilog(None, "alu.v", false).unwrap().path;
    let alu_tb = project.add_verilog(None, "alu_tb.v", false).unwrap().path;
    let mux_tb = project.add_verilog(None, "mux_tb.v", false).unwrap().path;

    assert!(project.remove_verilog(&alu).unwrap());
    assert_eq!(project.top_level(), None);
    assert_eq!(project.top_module().unwrap(), None);
    assert!(alu.is_file(), "remover do projeto não apaga do disco");
    assert!(!project.remove_verilog(&alu).unwrap());
    assert!(!project.remove_verilog("nunca_existiu.v").unwrap());

    // Sem o escolhido, o testbench volta a ser o primeiro da lista.
    assert!(project.remove_verilog(&alu_tb).unwrap());
    let reopened = Project::open(project.spf_path()).unwrap();
    assert_eq!(reopened.testbench(), Some(mux_tb));
    assert!(reopened.files(FileRole::Synthesizable).is_empty());
    assert_eq!(reopened.files(FileRole::Testbench).len(), 1);
}

#[test]
fn set_top_by_file_or_by_module_name() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let alu = project.add_verilog(None, "alu.v", false).unwrap().path;
    // Um arquivo cujo módulo não tem o nome dele.
    let regs = write(
        root.join("rtl/regs.v"),
        "module banco (input wire [7:0] d, output wire [7:0] q);\n    assign q = d;\nendmodule\n",
    );
    project.add_verilog(None, &regs, false).unwrap();

    assert_eq!(project.set_top("banco").unwrap(), regs);
    assert_eq!(project.top_level(), Some(regs.clone()));
    assert_eq!(project.top_module().unwrap().as_deref(), Some("banco"));

    assert_eq!(project.set_top(alu.as_str()).unwrap(), alu);
    assert_eq!(project.top_module().unwrap().as_deref(), Some("alu"));
    assert_eq!(project.set_top("rtl/regs.v").unwrap(), regs);

    // Um arquivo que existe e não estava registrado: entra como sintetizável.
    let extra = write(root.join("extra.v"), AND_GATE);
    assert_eq!(project.set_top("extra.v").unwrap(), extra);
    assert!(
        project
            .files(FileRole::Synthesizable)
            .iter()
            .any(|f| f.path == extra)
    );
    let reopened = Project::open(project.spf_path()).unwrap();
    assert_eq!(reopened.top_level(), Some(extra));
}

#[test]
fn set_top_with_unknown_module_lists_the_available_ones() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let alu = project.add_verilog(None, "alu.v", false).unwrap().path;
    let regs = write(
        root.join("regs.v"),
        "module banco (input wire [7:0] d, output wire [7:0] q);\n    assign q = d;\nendmodule\n",
    );
    project.add_verilog(None, &regs, false).unwrap();
    project.add_verilog(None, "alu_tb.v", false).unwrap();

    let err = project.set_top("nao_existe").unwrap_err();
    assert_eq!(err.code(), "module_not_found");
    let text = err.to_string();
    match err {
        LaceError::ModuleNotFound {
            name,
            mut available,
        } => {
            assert_eq!(name, "nao_existe");
            available.sort();
            assert_eq!(available, ["alu", "banco"]);
        }
        other => panic!("{other:?}"),
    }
    assert!(text.contains("alu") && text.contains("banco"), "{text}");

    // Só os sintetizáveis contam: o módulo do testbench não vira topo.
    match project.set_top("alu_tb").unwrap_err() {
        LaceError::ModuleNotFound { available, .. } => {
            assert!(!available.iter().any(|m| m == "alu_tb"), "{available:?}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(project.top_level(), Some(alu), "erro não muda o topo");
}

#[test]
fn top_module_of_a_file_with_several_modules() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    // Vários módulos: vale o que tem o nome do arquivo.
    let chip = write(
        root.join("chip.v"),
        "module helper (input wire a, output wire y);\n    assign y = a;\nendmodule\n\
         module chip (input wire a, output wire y);\n    helper h (.a(a), .y(y));\nendmodule\n",
    );
    project.add_verilog(None, &chip, false).unwrap();
    assert_eq!(project.top_module().unwrap().as_deref(), Some("chip"));

    // Nenhum com o nome do arquivo: não há como escolher.
    let misc = write(
        root.join("misc.v"),
        "module alpha (input wire a, output wire y);\n    assign y = a;\nendmodule\n\
         module beta (input wire a, output wire y);\n    assign y = ~a;\nendmodule\n",
    );
    project.add_verilog(None, &misc, false).unwrap();
    project.set_top("misc.v").unwrap();
    let err = project.top_module().unwrap_err();
    assert_eq!(err.code(), "module_not_found", "{err}");

    // A mesma regra vale para o testbench escolhido.
    let bench = write(
        root.join("bench_tb.v"),
        "module estimulo_aux;\nendmodule\nmodule bench_tb;\n    initial #1 $finish;\nendmodule\n",
    );
    project.add_verilog(None, &bench, false).unwrap();
    assert_eq!(
        project.testbench_module().unwrap().as_deref(),
        Some("bench_tb")
    );
}

#[test]
fn unregistered_verilog_skips_registered_hidden_and_processors() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    project.add_verilog(None, "rtl/a.v", false).unwrap();
    project.add_verilog(None, "rtl/a_tb.v", false).unwrap();
    write(root.join("rtl/b.v"), AND_GATE);
    write(root.join("c.sv"), SV_PORTS);
    write(root.join("notas.txt"), "nada\n");
    write(root.join(".oculto/d.v"), AND_GATE);
    write(root.join(".lace/Temp/e.v"), AND_GATE);
    let soma = project
        .add_processor(&NewProcessor::new("soma", Language::Cmm))
        .unwrap()
        .clone();
    write(soma.hardware_dir().join("soma.v"), AND_GATE);

    let mut found = project.unregistered_verilog();
    found.sort();
    assert_eq!(found, [root.join("c.sv"), root.join("rtl/b.v")]);
}

#[test]
fn waveform_path_before_simulating() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let err = waveform_path(&project, None).unwrap_err();
    assert_eq!(err.code(), "no_testbench", "{err}");

    // O `$dumpfile` é relativo à raiz, onde a simulação roda.
    let tb = write(
        root.join("rtl/meu_tb.v"),
        "\
module meu_tb;
    initial begin
        $dumpfile(\"saida.vcd\");
        $dumpvars(0, meu_tb);
        #1 $finish;
    end
endmodule
",
    );
    project.add_verilog(None, &tb, false).unwrap();
    // O Icarus grava sempre FST: `saida.fst`, enquanto não há onda nenhuma.
    assert_eq!(
        waveform_path(&project, None).unwrap(),
        root.join("saida.fst")
    );
    // A do Verilator (`.vcd`) vale quando é a única, ou a mais nova.
    write(root.join("saida.vcd"), "$enddefinitions $end\n");
    assert_eq!(
        waveform_path(&project, None).unwrap(),
        root.join("saida.vcd")
    );
    std::thread::sleep(std::time::Duration::from_millis(50));
    write(root.join("saida.fst"), "");
    assert_eq!(
        waveform_path(&project, None).unwrap(),
        root.join("saida.fst")
    );

    // Sem `$dumpfile`: `<módulo do testbench>.fst` na raiz (Icarus).
    let bench = write(root.join("rtl/estimulo.v"), BANCADA);
    project.set_testbench(&bench).unwrap();
    assert_eq!(
        waveform_path(&project, None).unwrap(),
        root.join("bancada.fst")
    );

    // Processador ainda não compilado: não há testbench gerado.
    let soma = project
        .add_processor(&NewProcessor::new("soma", Language::Cmm))
        .unwrap()
        .clone();
    let err = waveform_path(&project, Some(&soma)).unwrap_err();
    assert!(matches!(err, LaceError::NotBuilt { .. }), "{err}");
}

#[test]
fn new_errors_have_stable_codes_and_neutral_messages() {
    let empty = LaceError::EmptyProject("/p/p.spf".into());
    assert_eq!(empty.code(), "empty_project");
    let missing = LaceError::ModuleNotFound {
        name: "cpu".into(),
        available: vec!["alu".into(), "banco".into()],
    };
    assert_eq!(missing.code(), "module_not_found");
    let text = missing.to_string();
    assert!(
        text.contains("cpu") && text.contains("alu, banco"),
        "{text}"
    );
    // As dicas com nomes de comando ficam na CLI.
    for err in [
        empty,
        missing,
        LaceError::NoTopLevel("/p/p.spf".into()),
        LaceError::NoTestbench("/p/p.spf".into()),
    ] {
        let text = err.to_string();
        assert!(!text.contains("lace "), "{text}");
    }
}

#[test]
fn top_can_be_any_verilog_but_a_testbench_name() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let gate = write(root.join("and_gate.v"), AND_GATE);
    let named_tb = write(root.join("and_gate_tb.v"), BANCADA);
    let bench = write(root.join("bancada.v"), BANCADA);
    project
        .add_file(FileRole::Synthesizable, &gate, None)
        .unwrap();
    project
        .add_file(FileRole::Testbench, &named_tb, None)
        .unwrap();
    project.add_file(FileRole::Testbench, &bench, None).unwrap();
    project.set_top_level(&gate).unwrap();
    let spf_before = read(project.spf_path());

    for name in ["and_gate_tb.v", "tb_x.v", "tb.v"] {
        write(root.join(name), AND_GATE);
        let err = project.set_top(name).unwrap_err();
        assert!(
            matches!(err, LaceError::InvalidName { .. }),
            "{name}: {err}"
        );
    }
    assert_eq!(read(project.spf_path()), spf_before, "nada mudou");

    // Qualquer outro .v, mesmo um testbench registrado (muda de lista) ou um
    // arquivo fora do .spf (é registrado).
    project.set_top("bancada.v").unwrap();
    assert_eq!(project.top_level(), Some(bench.clone()));
    assert!(
        project
            .files(FileRole::Testbench)
            .iter()
            .all(|f| f.path != bench)
    );
    let loose = write(root.join("rtl/solto.v"), AND_GATE);
    project.set_top("rtl/solto.v").unwrap();
    assert_eq!(project.top_level(), Some(loose));

    // add_verilog não escolhe sozinho um nome de testbench como topo.
    let (_guard2, mut other) = self::project();
    let module_named_tb = write(other.root().join("porta_tb.v"), AND_GATE);
    let added = other.add_verilog(None, &module_named_tb, false).unwrap();
    assert_eq!(added.role, FileRole::Synthesizable);
    assert!(!added.selected);
    assert_eq!(other.top_level(), None);
}

// ------------------------------------------------------------ mover

#[test]
fn move_path_keeps_order_selection_and_top_module() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let gate = write(root.join("and_gate.v"), AND_GATE);
    let mux = write(root.join("mux2.v"), MUX2);
    let bench = write(root.join("bancada.v"), BANCADA);
    project
        .add_file(FileRole::Synthesizable, &gate, None)
        .unwrap();
    project
        .add_file(FileRole::Synthesizable, &mux, None)
        .unwrap();
    project.add_file(FileRole::Testbench, &bench, None).unwrap();
    project.set_top("mux2").unwrap();
    project.set_testbench(&bench).unwrap();

    // Renomear o topo, numa pasta que ainda não existe.
    let moved = project.move_path("mux2.v", "rtl/mux.v").unwrap();
    let mux = root.join("rtl/mux.v");
    assert_eq!(moved.to, mux);
    assert_eq!(moved.files.len(), 1);
    assert!(moved.files[0].top_level, "{moved:#?}");
    assert!(!root.join("mux2.v").exists() && mux.is_file());
    let order: Vec<_> = project
        .files(FileRole::Synthesizable)
        .into_iter()
        .map(|f| f.path)
        .collect();
    assert_eq!(order, [gate.clone(), mux.clone()], "a ordem do .spf fica");
    assert_eq!(project.top_level(), Some(mux.clone()));
    assert_eq!(project.top_module().unwrap().as_deref(), Some("mux2"));

    // Uma pasta inteira; o testbench escolhido estava dentro dela.
    project.move_path("bancada.v", "rtl/bancada.v").unwrap();
    let moved = project
        .move_path(root.join("rtl"), root.join("hdl/rtl"))
        .unwrap();
    assert_eq!(moved.files.len(), 2, "{moved:#?}");
    let reopened = Project::open(project.spf_path()).unwrap();
    assert_eq!(reopened.top_level(), Some(root.join("hdl/rtl/mux.v")));
    assert_eq!(reopened.testbench(), Some(root.join("hdl/rtl/bancada.v")));
    assert_eq!(reopened.top_module().unwrap().as_deref(), Some("mux2"));
    // O .spf guarda o caminho relativo, com /, e o nome novo.
    let spf: serde_json::Value = serde_json::from_str(&read(project.spf_path())).unwrap();
    let entry = &spf["structure"]["synthesizableFiles"][1];
    assert_eq!(entry["path"], "hdl/rtl/mux.v");
    assert_eq!(entry["name"], "mux.v");
    assert_eq!(spf["structure"]["testbenchFile"], "hdl/rtl/bancada.v");

    // Um arquivo fora do .spf também se move.
    write(root.join("notas.txt"), "x");
    let moved = project.move_path("notas.txt", "docs/notas.txt").unwrap();
    assert!(moved.files.is_empty());
    assert!(root.join("docs/notas.txt").is_file());
}

#[test]
fn move_path_refuses_what_stays_in_place() {
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    project
        .add_processor(&NewProcessor::new("soma", Language::Cmm))
        .unwrap();
    write(root.join("rtl/a.v"), AND_GATE);
    write(root.join("b.v"), AND_GATE);
    std::fs::create_dir_all(root.join(".lace/Temp")).unwrap();
    let spf_before = read(project.spf_path());

    let spf = project.spf_path().to_owned();
    for (from, to) in [
        (spf.clone(), root.join("x.spf")),
        (root.join(".lace"), root.join("lace")),
        (root.join("soma"), root.join("outro")),
        (root.join("soma/Software"), root.join("Software")),
        (root.join("soma/Hardware"), root.join("Hardware")),
        (root.join("soma/Software/soma.cmm"), root.join("soma.cmm")),
        (root.join("rtl"), root.join("rtl/dentro")),
        (root.join("b.v"), root.join(".lace/b.v")),
    ] {
        let err = project.move_path(&from, &to).unwrap_err();
        assert!(matches!(err, LaceError::CannotMove { .. }), "{from}: {err}");
        assert_eq!(err.code(), "cannot_move");
    }
    let err = project.move_path("b.v", "rtl/a.v").unwrap_err();
    assert!(matches!(err, LaceError::PathExists(_)), "{err}");
    let (_outside_guard, outside) = common::tempdir();
    let err = project.move_path("b.v", outside.join("b.v")).unwrap_err();
    assert!(matches!(err, LaceError::OutsideProject(_)), "{err}");
    let err = project.move_path("nao_existe.v", "c.v").unwrap_err();
    assert!(matches!(err, LaceError::InvalidProject { .. }), "{err}");
    assert_eq!(read(project.spf_path()), spf_before, "nada mudou no .spf");
    assert!(root.join("b.v").is_file() && root.join("soma/Software/soma.cmm").is_file());

    // O resto da pasta de um processador pode mudar de nome.
    write(root.join("soma/Simulation/input_0.txt"), "1\n");
    project
        .move_path("soma/Simulation/input_0.txt", "soma/Simulation/input_1.txt")
        .unwrap();
    // Mover para o mesmo lugar não faz nada.
    let same = project.move_path("b.v", "b.v").unwrap();
    assert!(same.files.is_empty());
}

// ------------------------------------------------------------ com ferramentas

#[test]
fn check_of_empty_project_is_an_error() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, mut project) = project();
    let err = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert!(matches!(err, LaceError::EmptyProject(_)), "{err}");

    // Registrar e tirar volta ao projeto vazio.
    let gate = project.add_verilog(None, "and_gate.v", false).unwrap().path;
    project.remove_verilog(&gate).unwrap();
    let err = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "empty_project");
}

#[test]
fn yosys_reads_ansi_and_non_ansi_ports() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Yosys]) else {
        return;
    };
    let (_guard, dir) = common::tempdir();
    let ansi = write(dir.join("ansi.v"), ANSI_DUT);
    let non_ansi = write(dir.join("nonansi.v"), NON_ANSI);
    let sv = write(dir.join("sv_ports.sv"), SV_PORTS);
    let interfaces = read_interfaces(
        Some(&toolchain),
        &[ansi.clone(), non_ansi.clone(), sv.clone()],
    )
    .unwrap();
    let find = |name: &str| {
        interfaces
            .iter()
            .find(|i| i.name == name)
            .unwrap_or_else(|| panic!("{name} não lido: {interfaces:#?}"))
    };
    assert_eq!(interfaces.len(), 4, "{interfaces:#?}");

    let dut = find("ansi_dut");
    assert_eq!(dut.file, ansi);
    // `[WIDTH-1:0]` com o parâmetro no valor padrão.
    assert_eq!(
        ports(dut),
        [
            ("clk", Input, 1, false),
            ("rst_n", Input, 1, false),
            ("din", Input, 8, false),
            ("sdin", Input, 8, true),
            ("dout", Output, 8, false),
            ("valid", Output, 1, false),
            ("bus", Inout, 4, false),
            ("little", Input, 4, false),
        ]
    );

    let dut = find("nonansi_dut");
    assert_eq!(dut.file, non_ansi);
    assert_eq!(
        ports(dut),
        [
            ("clk", Input, 1, false),
            ("a", Input, 4, false),
            ("b", Input, 8, false),
            ("q", Output, 5, false),
            ("io", Inout, 1, false),
        ]
    );
    // A ordem é a do cabeçalho, não a das declarações.
    assert_eq!(
        ports(find("ord")),
        [
            ("zz", Output, 1, false),
            ("bb", Input, 4, true),
            ("aa", Input, 8, false),
        ]
    );
    let dut = find("sv_ports");
    assert_eq!(dut.file, sv);
    assert_eq!(
        ports(dut),
        [
            ("clk", Input, 1, false),
            ("a", Input, 8, false),
            ("y", Output, 8, false),
        ]
    );
}

#[test]
fn testbench_from_yosys_ports_elaborates() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Yosys]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let ord = write(root.join("ord.v"), NON_ANSI);
    project.add_verilog(Some(&toolchain), &ord, false).unwrap();
    let tb = project
        .add_verilog(Some(&toolchain), "ord_tb.v", false)
        .unwrap();
    assert!(tb.created);
    assert!(instantiates(&read(&tb.path), "ord"));

    let result = check(
        &toolchain,
        &project,
        &check_options(Some(&tb.path), false),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    assert_eq!(result.targets, ["ord_tb"]);
    // Larguras certas: o Icarus não completa nem corta porta nenhuma.
    assert!(
        result
            .diagnostics
            .iter()
            .all(|d| !d.message.contains("expects")),
        "{:#?}",
        result.diagnostics
    );
}

#[test]
fn verilog_flow_from_scratch() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, parent) = common::tempdir();
    let mut project = Project::create(&parent, "porta").unwrap();
    let root = project.root().to_owned();

    let module = project
        .add_verilog(Some(&toolchain), root.join("rtl/and_gate.v"), false)
        .unwrap();
    assert_eq!(
        (module.role, module.created, module.selected),
        (FileRole::Synthesizable, true, true)
    );
    let tb = project
        .add_verilog(Some(&toolchain), root.join("rtl/and_gate_tb.v"), false)
        .unwrap();
    assert_eq!(
        (tb.role, tb.created, tb.selected),
        (FileRole::Testbench, true, true)
    );
    let tb_text = read(&tb.path);

    // Verificar: o design inteiro sem `-s` e depois o testbench.
    let checked = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(checked.succeeded(), "{checked:#?}");
    assert_eq!(checked.failed_step, None);
    for target in ["and_gate", "and_gate_tb"] {
        assert!(
            checked.targets.iter().any(|t| t == target),
            "{:?}",
            checked.targets
        );
    }
    let syntax = steps(&checked, Step::CheckSyntax);
    assert_eq!(syntax.len(), 2, "{syntax:#?}");
    for step in &syntax {
        for flag in ["-tnull", "-Wall"] {
            assert!(has_arg(step, flag), "{flag}: {:?}", step.command.args);
        }
        // Só `.v`: sem `-g2012`, que reservaria `bit`, `logic` e afins.
        assert!(!has_arg(step, "-g2012"), "{:?}", step.command.args);
    }
    assert!(roots(syntax[0]).is_empty(), "{:?}", syntax[0].command.args);
    assert_eq!(roots(syntax[1]), ["and_gate_tb"]);
    assert!(steps(&checked, Step::Lint).is_empty());
    assert!(!uses_sapho_library(&checked.steps));
    assert!(
        checked
            .diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "{:#?}",
        checked.diagnostics
    );

    // Simular: a onda sai na raiz, onde `waveform_path` disse.
    let expected = waveform_path(&project, None).unwrap();
    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert!(sim.succeeded(), "{sim:#?}");
    assert_eq!(sim.top, "and_gate_tb");
    assert!(!uses_sapho_library(&sim.steps));
    let vvp = sim.steps.iter().find(|s| s.step == Step::Simulate).unwrap();
    assert!(has_arg(vvp, "-n"), "{:?}", vvp.command.args);

    let wave = sim.waveform.unwrap();
    assert_eq!(wave.path, expected);
    assert_eq!(wave.path.parent(), Some(root.as_path()));
    assert!(wave.path.is_file());
    if wave.path.extension() == Some("fst") {
        assert_eq!(wave.format, WaveformFormat::Fst);
        assert!(common::is_fst(&wave.path));
    } else {
        assert_eq!(wave.format, WaveformFormat::Vcd);
        assert!(!common::is_fst(&wave.path));
    }
    // Profundidade 0: os sinais dentro do and_gate também estão na onda.
    assert!(
        common::dumped_scopes(&wave.path) >= 2,
        "a onda só tem o testbench"
    );
    assert_eq!(read(&tb.path), tb_text, "o testbench do usuário não muda");
}

/// Um `include` relativo à pasta de quem o faz: `rtl/cpu.v` inclui
/// `defs.vh` de `rtl/`, e o testbench, em `tb/`, inclui `vetores.vh` de
/// `tb/`. No Windows o Icarus só os acha com os caminhos em `/`; e o
/// testbench sem `$dumpfile` é compilado numa cópia em `.lace/Temp`, que
/// precisa da pasta do original nos `include`.
#[test]
fn include_next_to_the_including_file() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    write(root.join("rtl/defs.vh"), "`define LARGURA 4\n");
    let cpu = write(
        root.join("rtl/cpu.v"),
        "`include \"defs.vh\"\n\
         module cpu (input wire [`LARGURA-1:0] a, output wire [`LARGURA-1:0] y);\n\
         \x20   assign y = ~a;\n\
         endmodule\n",
    );
    write(
        root.join("tb/vetores.vh"),
        "localparam [3:0] ENTRADA = 4'b0101;\n",
    );
    // O `timescale` só no testbench faz o Icarus avisar dos módulos sem
    // ele, com o arquivo numa linha de continuação ("declared here").
    let tb = write(
        root.join("tb/cpu_tb.v"),
        "`timescale 1ns/1ps\n\
         module cpu_tb;\n\
         \x20   `include \"vetores.vh\"\n\
         \x20   reg [3:0] a = ENTRADA;\n\
         \x20   wire [3:0] y;\n\
         \x20   cpu dut (.a(a), .y(y));\n\
         \x20   initial begin\n\
         \x20       #1 $display(\"y = %b\", y);\n\
         \x20       $finish;\n\
         \x20   end\n\
         endmodule\n",
    );
    let cpu = project.add_verilog(Some(&toolchain), &cpu, false).unwrap();
    assert_eq!(cpu.role, FileRole::Synthesizable);
    let tb = project.add_verilog(Some(&toolchain), &tb, false).unwrap();
    assert_eq!(tb.role, FileRole::Testbench);

    let checked = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(checked.succeeded(), "{checked:#?}");
    // Todo arquivo de diagnóstico vem com o separador do sistema, inclusive
    // o do aviso de `timescale`, que o Icarus escreve com `/`.
    let timescale = checked
        .diagnostics
        .iter()
        .find(|d| d.message.contains("timescale"))
        .unwrap_or_else(|| panic!("{:#?}", checked.diagnostics));
    assert_eq!(
        timescale.file.as_ref().map(|f| f.as_str()),
        Some(cpu.path.as_str()),
        "{timescale:#?}"
    );

    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert!(sim.succeeded(), "{sim:#?}");
    let vvp = sim.steps.iter().find(|s| s.step == Step::Simulate).unwrap();
    assert!(vvp.stdout.contains("y = 1010"), "{}", vvp.stdout);

    let tree = hierarchy(
        &toolchain,
        &project,
        &HierarchyOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(tree.status, Status::Succeeded, "{tree:#?}");
    // O arquivo de cada módulo vem como o projeto o chama, com o separador
    // do sistema, embora o Icarus o tenha recebido com `/`.
    let design = tree.design.as_ref().expect("a elaboração do design");
    let node = design.roots.iter().find(|m| m.module == "cpu").unwrap();
    assert_eq!(
        node.file.as_ref().map(|f| f.as_str()),
        Some(cpu.path.as_str())
    );
}

/// O diagnóstico aponta o arquivo com o mesmo nome que o projeto usa, com o
/// separador do sistema: o Studio abre o arquivo do erro pelo caminho, e um
/// nome com `/` no Windows viraria outra aba do mesmo arquivo.
#[test]
fn diagnostics_name_the_file_as_the_project_does() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let bad = write(
        root.join("rtl/ruim.v"),
        "module ruim (input wire a, output wire y);\n    assign y = a\nendmodule\n",
    );
    let bad = project.add_verilog(Some(&toolchain), &bad, false).unwrap();
    let checked = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(!checked.succeeded(), "{checked:#?}");
    let error = checked
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap_or_else(|| panic!("{:#?}", checked.diagnostics));
    assert_eq!(
        error.file.as_ref().map(|f| f.as_str()),
        Some(bad.path.as_str()),
        "{error:#?}"
    );
}

/// `and_gate` (topo) e `orphan`, que ninguém instancia e não elabora.
fn project_with_orphan(toolchain: &lace_core::Toolchain) -> (tempfile::TempDir, Project) {
    let (guard, mut project) = project();
    let root = project.root().to_owned();
    for (name, text) in [
        ("and_gate.v", AND_GATE),
        ("mux2.v", MUX2),
        ("orphan.v", ORPHAN),
    ] {
        let file = write(root.join("rtl").join(name), text);
        let added = project.add_verilog(Some(toolchain), &file, false).unwrap();
        assert_eq!(added.role, FileRole::Synthesizable, "{name}");
    }
    assert_eq!(project.top_module().unwrap().as_deref(), Some("and_gate"));
    (guard, project)
}

#[test]
fn check_elaborates_modules_outside_the_top() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, project) = project_with_orphan(&toolchain);
    let orphan = project.root().join("rtl/orphan.v");

    let result = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed, "{result:#?}");
    assert_eq!(result.failed_step, Some(Step::CheckSyntax));
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(orphan.as_path()));
    assert_eq!(error.line, Some(2));
}

#[test]
fn check_restricted_to_one_file() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, mut project) = project_with_orphan(&toolchain);
    let root = project.root().to_owned();

    // O mux2 usa o and_gate de outro arquivo; o orphan nem é elaborado.
    let mux2 = root.join("rtl/mux2.v");
    let result = check(
        &toolchain,
        &project,
        &check_options(Some(&mux2), false),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    assert_eq!(result.targets, ["mux2"]);
    let elaborated: Vec<_> = steps(&result, Step::CheckSyntax)
        .into_iter()
        .flat_map(roots)
        .collect();
    assert_eq!(elaborated, ["mux2"]);

    let orphan = root.join("rtl/orphan.v");
    let result = check(
        &toolchain,
        &project,
        &check_options(Some(&orphan), false),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.targets, ["orphan"]);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.file.as_deref() == Some(orphan.as_path()))
    );

    // Um testbench: só ele como raiz, com o design.
    let tb = project
        .add_verilog(Some(&toolchain), "rtl/and_gate_tb.v", false)
        .unwrap();
    let result = check(
        &toolchain,
        &project,
        &check_options(Some(&tb.path), false),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    assert_eq!(result.targets, ["and_gate_tb"]);
}

#[test]
fn check_reports_testbench_syntax_error() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let gate = write(root.join("rtl/and_gate.v"), AND_GATE);
    let tb = write(root.join("rtl/and_gate_tb.v"), BROKEN_TB);
    project.add_verilog(Some(&toolchain), &gate, false).unwrap();
    assert_eq!(
        project
            .add_verilog(Some(&toolchain), &tb, false)
            .unwrap()
            .role,
        FileRole::Testbench
    );

    let result = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed, "{result:#?}");
    assert_eq!(result.failed_step, Some(Step::CheckSyntax));
    // O design passa; quem falha é a elaboração do testbench.
    let syntax = steps(&result, Step::CheckSyntax);
    assert!(syntax[0].termination.success(), "{:#?}", syntax[0]);
    let error = result
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!(error.file.as_deref(), Some(tb.as_path()));
    assert_eq!(error.line, Some(6));

    let result = check(
        &toolchain,
        &project,
        &check_options(Some(&tb), false),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed);
}

#[test]
fn lint_reports_width_mismatch() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Verilator]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let widthy = write(root.join("widthy.v"), WIDTHY);
    project
        .add_verilog(Some(&toolchain), &widthy, false)
        .unwrap();

    // O Icarus aceita a atribuição de 8 bits a 4 sem dizer nada.
    let plain = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(plain.succeeded(), "{plain:#?}");
    assert!(steps(&plain, Step::Lint).is_empty());

    let linted = check(
        &toolchain,
        &project,
        &check_options(None, true),
        &Control::default(),
    )
    .unwrap();
    // Aviso não reprova (`-Wno-fatal`).
    assert!(linted.succeeded(), "{linted:#?}");
    let lint = steps(&linted, Step::Lint);
    assert_eq!(lint.len(), 1);
    let args = &lint[0].command.args;
    for flag in ["--lint-only", "-Wall"] {
        assert!(args.iter().any(|a| a == flag), "{flag}: {args:?}");
    }
    let top = args.iter().position(|a| a == "--top-module").unwrap();
    assert_eq!(args[top + 1], "widthy");
    assert_ne!(lint[0].command.cwd, root, "o lint roda numa pasta vazia");

    let warning = linted
        .diagnostics
        .iter()
        .find(|d| d.tool == Tool::Verilator && d.message.contains("WIDTH"))
        .unwrap_or_else(|| panic!("{:#?}", linted.diagnostics));
    assert_eq!(warning.severity, Severity::Warning);
    assert_eq!(warning.file.as_deref(), Some(widthy.as_path()));
    assert_eq!(warning.line, Some(5));
}

#[test]
fn simulation_fails_on_error_task() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    project
        .add_verilog(
            Some(&toolchain),
            write(root.join("and_gate.v"), AND_GATE),
            false,
        )
        .unwrap();
    project
        .add_verilog(
            Some(&toolchain),
            write(root.join("and_gate_tb.v"), FAILING_TB),
            false,
        )
        .unwrap();

    let result = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed, "{result:#?}");
    assert_eq!(result.failed_step, Some(Step::Simulate));
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.message.contains("y deveria ser 1")),
        "{:#?}",
        result.diagnostics
    );
}

#[test]
fn testbench_module_comes_from_contents() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    project
        .add_verilog(
            Some(&toolchain),
            write(root.join("and_gate.v"), AND_GATE),
            false,
        )
        .unwrap();
    let bench = write(root.join("rtl/estimulo.v"), BANCADA);
    project
        .add_verilog(Some(&toolchain), &bench, false)
        .unwrap();
    assert_eq!(
        project.testbench_module().unwrap().as_deref(),
        Some("bancada")
    );
    let expected = waveform_path(&project, None).unwrap();
    assert_eq!(expected, root.join("bancada.fst"));

    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert!(sim.succeeded(), "{sim:#?}");
    assert_eq!(sim.top, "bancada");
    let run = sim.steps.iter().find(|s| s.step == Step::Simulate).unwrap();
    assert!(run.stdout.contains("y = 1"), "{}", run.stdout);

    // O dump injetado: FST, na raiz, com o and_gate dentro.
    let wave = sim.waveform.unwrap();
    assert_eq!(wave.path, expected);
    assert_eq!(wave.format, WaveformFormat::Fst);
    assert!(common::is_fst(&wave.path));
    assert!(common::dumped_scopes(&wave.path) >= 2);
    assert_eq!(read(&bench), BANCADA, "o testbench do usuário não muda");
}

#[test]
fn processor_waveform_path_matches_simulation() {
    let Some(toolchain) = common::toolchain_with(&[
        Tool::Cmmcomp,
        Tool::Appcomp,
        Tool::Asmcomp,
        Tool::Iverilog,
        Tool::Vvp,
    ]) else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let soma = project.require_processor("soma").unwrap();
    let err = waveform_path(&project, Some(soma)).unwrap_err();
    assert!(matches!(err, LaceError::NotBuilt { .. }), "{err}");

    let built = build(
        &toolchain,
        soma,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(built.succeeded(), "{built:#?}");
    let expected = waveform_path(&project, Some(soma)).unwrap();
    assert_eq!(expected, soma.temp_dir.join("soma_tb.fst"));

    let sim = simulate(
        &toolchain,
        soma,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert!(sim.succeeded(), "{sim:#?}");
    let wave = sim.waveform.unwrap();
    assert_eq!(wave.path, expected);
    assert_eq!(wave.format, WaveformFormat::Fst);
}

#[test]
fn verilog_only_project_skips_sapho_library() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog, Tool::Vvp]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    for (name, text) in [
        ("processor.v", USER_PROCESSOR),
        ("processor_tb.v", USER_PROCESSOR_TB),
    ] {
        project
            .add_verilog(Some(&toolchain), write(root.join(name), text), false)
            .unwrap();
    }
    assert!(project.processors().is_empty());
    let lint = toolchain.tool(Tool::Verilator).is_ok();

    let checked = check(
        &toolchain,
        &project,
        &check_options(None, lint),
        &Control::default(),
    )
    .unwrap();
    assert!(checked.succeeded(), "{checked:#?}");
    assert!(!uses_sapho_library(&checked.steps), "{:#?}", checked.steps);

    let sim = simulate_project(
        &toolchain,
        &project,
        &SimulationOptions::new(Simulator::Icarus),
        &Control::default(),
    )
    .unwrap();
    assert!(sim.succeeded(), "{sim:#?}");
    assert!(!uses_sapho_library(&sim.steps), "{:#?}", sim.steps);
    let run = sim.steps.iter().find(|s| s.step == Step::Simulate).unwrap();
    assert!(run.stdout.contains("y = 10"), "{}", run.stdout);
}

#[test]
fn verilog_only_synthesis_skips_sapho_library() {
    // O Yosys lê a biblioteca inteira, e um `processor` do usuário colidiria
    // com o dela (`Re-definition of module`).
    let Some(toolchain) = common::toolchain_with(&[Tool::Yosys]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    project
        .add_verilog(
            Some(&toolchain),
            write(root.join("processor.v"), USER_PROCESSOR),
            false,
        )
        .unwrap();
    let synth = synthesize(
        &toolchain,
        &project,
        &DesignTarget::TopLevel,
        &Control::default(),
    )
    .unwrap();
    assert!(synth.succeeded(), "{:#?}", synth.diagnostics);
    assert!(synth.modules.iter().any(|m| m == "processor"));
}

#[test]
fn generated_processor_verilog_can_be_the_top() {
    let Some(toolchain) = common::toolchain_with(&[
        Tool::Cmmcomp,
        Tool::Appcomp,
        Tool::Asmcomp,
        Tool::Iverilog,
        Tool::Yosys,
    ]) else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let mut project = Project::open(&root).unwrap();
    let soma = project.require_processor("soma").unwrap().clone();
    let built = build(
        &toolchain,
        &soma,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(built.succeeded(), "{built:#?}");

    let verilog = soma.hardware_dir().join("soma.v");
    project.set_top(verilog.as_str()).unwrap();
    assert_eq!(project.top_level(), Some(verilog.clone()));
    assert_eq!(project.top_module().unwrap().as_deref(), Some("soma"));

    // Registrado e também entrando como Verilog do processador: as operações
    // não o leem duas vezes.
    let checked = check(
        &toolchain,
        &project,
        &CheckOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(checked.succeeded(), "{checked:#?}");
    let synth = synthesize(
        &toolchain,
        &project,
        &DesignTarget::TopLevel,
        &Control::default(),
    )
    .unwrap();
    assert!(synth.succeeded(), "{:#?}", synth.diagnostics);
    assert_eq!(synth.top, "soma");
    let tree = hierarchy(
        &toolchain,
        &project,
        &HierarchyOptions::default(),
        &Control::default(),
    )
    .unwrap();
    let design = tree.design.unwrap();
    assert_eq!(
        design.roots.iter().filter(|r| r.module == "soma").count(),
        1
    );
}

// ------------------------------------------------------------ hierarquia

/// O nó de `module` em qualquer profundidade.
fn find<'a>(nodes: &'a [ModuleInstance], module: &str) -> Option<&'a ModuleInstance> {
    nodes.iter().find_map(|n| {
        if n.module == module {
            Some(n)
        } else {
            find(&n.children, module)
        }
    })
}

#[test]
fn hierarchy_of_design_and_each_testbench() {
    let Some(toolchain) = common::toolchain_with(&[Tool::Iverilog]) else {
        return;
    };
    let (_guard, mut project) = project();
    let root = project.root().to_owned();
    let gate = write(root.join("rtl/and_gate.v"), AND_GATE);
    let mux = write(root.join("rtl/mux2.v"), MUX2);
    let bench = write(root.join("rtl/bancada.v"), BANCADA);
    project
        .add_file(FileRole::Synthesizable, &gate, None)
        .unwrap();
    project
        .add_file(FileRole::Synthesizable, &mux, None)
        .unwrap();
    project.add_file(FileRole::Testbench, &bench, None).unwrap();

    let result = hierarchy(
        &toolchain,
        &project,
        &HierarchyOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    assert!(
        result
            .steps
            .iter()
            .all(|s| s.step == Step::Elaborate && s.tool == Tool::Iverilog)
    );
    assert_eq!(result.steps.len(), 2);
    assert!(
        result
            .artifacts
            .iter()
            .all(|a| a.kind == ArtifactKind::IcarusImage && a.fresh),
        "{:#?}",
        result.artifacts
    );

    // O design: mux2 é a raiz, and_gate aparece duas vezes, na ordem do fonte.
    let design = result.design.as_ref().unwrap();
    assert_eq!(design.status, Status::Succeeded);
    let names: Vec<&str> = design.roots.iter().map(|r| r.module.as_str()).collect();
    assert_eq!(names, ["mux2"]);
    let top = &design.roots[0];
    assert_eq!((top.file.as_ref(), top.line), (Some(&mux), Some(1)));
    let children: Vec<(&str, &str, Option<u32>)> = top
        .children
        .iter()
        .map(|c| (c.name.as_str(), c.module.as_str(), c.instance_line))
        .collect();
    assert_eq!(
        children,
        [("u0", "and_gate", Some(8)), ("u1", "and_gate", Some(9))]
    );
    assert_eq!(top.children[0].file.as_ref(), Some(&gate));
    assert_eq!(top.children[0].instance_file.as_ref(), Some(&mux));
    assert!(!top.children[0].library);

    // O testbench, com o que ele instancia.
    let tb = &result.testbenches[0];
    assert_eq!(tb.testbench.as_ref(), Some(&bench));
    assert_eq!(tb.roots[0].module, "bancada");
    assert_eq!(tb.roots[0].children[0].name, "dut");

    // Um testbench quebrado não esconde o resto.
    let broken = write(root.join("rtl/and_gate_tb.v"), BROKEN_TB);
    project
        .add_file(FileRole::Testbench, &broken, None)
        .unwrap();
    let result = hierarchy(
        &toolchain,
        &project,
        &HierarchyOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert_eq!(result.status, Status::Failed);
    assert_eq!(result.failed_step, Some(Step::Elaborate));
    assert_eq!(result.design.as_ref().unwrap().roots.len(), 1);
    let [good, bad] = result.testbenches.as_slice() else {
        panic!("{result:#?}");
    };
    assert_eq!(good.status, Status::Succeeded);
    assert_eq!(bad.status, Status::Failed);
    assert!(bad.roots.is_empty());
    let error = bad
        .diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .unwrap();
    assert_eq!((error.file.as_ref(), error.line), (Some(&broken), Some(6)));
    assert!(result.diagnostics.contains(error));
}

#[test]
fn hierarchy_of_a_processor_reaches_the_sapho_library() {
    let Some(toolchain) =
        common::toolchain_with(&[Tool::Cmmcomp, Tool::Appcomp, Tool::Asmcomp, Tool::Iverilog])
    else {
        return;
    };
    let (_guard, root) = common::example("soma");
    let project = Project::open(&root).unwrap();
    let soma = project.require_processor("soma").unwrap();
    let built = build(
        &toolchain,
        soma,
        &BuildOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(built.succeeded(), "{built:#?}");

    // O projeto: o Verilog do processador é design, e o testbench que o
    // build gerou entra; o filtro (C), nunca compilado, fica de fora.
    let result = hierarchy(
        &toolchain,
        &project,
        &HierarchyOptions::default(),
        &Control::default(),
    )
    .unwrap();
    assert!(result.succeeded(), "{result:#?}");
    let design = result.design.as_ref().unwrap();
    let node = find(&design.roots, "soma").expect("o módulo soma");
    assert!(!node.library);
    let processor = find(&node.children, "processor").expect("o processor da biblioteca");
    assert!(processor.library, "{processor:#?}");
    let [tb] = result.testbenches.as_slice() else {
        panic!("{:#?}", result.testbenches);
    };
    assert_eq!(tb.processor.as_deref(), Some("soma"));
    assert_eq!(tb.roots[0].module, "soma_tb");
    assert!(find(&tb.roots, "soma").is_some());

    // Só o processador.
    let mut options = HierarchyOptions::default();
    options.processor = Some("soma".into());
    let only = hierarchy(&toolchain, &project, &options, &Control::default()).unwrap();
    assert!(only.succeeded(), "{only:#?}");
    assert_eq!(only.design.unwrap().roots[0].module, "soma");
    options.processor = Some("filtro".into());
    let err = hierarchy(&toolchain, &project, &options, &Control::default()).unwrap_err();
    assert!(matches!(err, LaceError::NotBuilt { .. }), "{err}");
}
