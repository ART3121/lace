//! O que o Quartus conta depois de compilar: os recursos da FPGA que o
//! design usa (o `<revisão>.fit.summary` do Fitter) e o tempo de cada clock
//! (as tabelas do `<revisão>.sta.rpt` do Timing Analyzer).
//!
//! Os formatos foram conferidos em relatórios publicados, não numa
//! compilação do Lace: o resumo do Fitter do Quartus II 10.1 e as tabelas do
//! Timing Analyzer do Quartus Prime 18.1, as mesmas desde o TimeQuest do
//! Quartus II 14.0. O que não der para ler fica de fora, sem erro: o
//! relatório completo continua na pasta da placa.

use schemars::JsonSchema;
use serde::Serialize;

/// Um recurso da FPGA no resumo do Fitter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct ResourceUsage {
    /// O nome, como o Quartus escreve (`Total logic elements`, `Total pins`,
    /// `Logic utilization (in ALMs)`).
    pub name: String,
    /// Quanto o design usa.
    pub used: u64,
    /// Quanto a FPGA tem, quando o Quartus diz.
    pub available: Option<u64>,
    /// Detalhe do recurso de cima (`Dedicated logic registers`, dentro de
    /// `Total logic elements`).
    pub detail: bool,
}

/// O tempo de um clock, no pior dos cantos de operação (tensão e
/// temperatura) que o Timing Analyzer analisa.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct ClockTiming {
    /// O clock: o sinal da placa, pelo nome do `create_clock`, ou o que o
    /// Quartus achou que é clock (um registrador que dirige um `always`).
    pub clock: String,
    /// A frequência do oscilador da placa, em MHz: a que o `.sdc` pede.
    /// `None` num clock que não é da placa.
    pub target_mhz: Option<f64>,
    /// A maior frequência em que o design funciona com esse clock (a
    /// `Restricted Fmax`), em MHz.
    pub fmax_mhz: Option<f64>,
    /// A folga de setup, em ns. Negativa: o design não alcança a frequência
    /// do clock.
    pub setup_slack_ns: Option<f64>,
    /// A folga de hold, em ns.
    pub hold_slack_ns: Option<f64>,
}

/// O tempo do design.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
#[non_exhaustive]
pub struct TimingSummary {
    /// Um por clock, na ordem em que o relatório os cita.
    pub clocks: Vec<ClockTiming>,
    /// Nenhuma folga de setup ou de hold é negativa.
    pub met: bool,
}

/// Os recursos do `<revisão>.fit.summary`, na ordem: as linhas
/// `nome : usado / total ( n % )` e `nome : usado`. As outras (o status, a
/// família, a versão) ficam de fora.
pub(crate) fn resources(text: &str) -> Vec<ResourceUsage> {
    text.lines()
        .filter_map(|line| {
            let detail = line.starts_with(char::is_whitespace);
            let (name, value) = line.trim().split_once(" : ")?;
            let (used, available) = match value.split_once(" / ") {
                Some((used, rest)) => (used, Some(number(rest.split(" (").next()?)?)),
                None => (value, None),
            };
            Some(ResourceUsage {
                name: name.trim().to_owned(),
                used: number(used)?,
                available,
                detail,
            })
        })
        .collect()
}

/// `1,532` como número.
fn number(text: &str) -> Option<u64> {
    let digits: String = text.trim().chars().filter(|&c| c != ',').collect();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// O tempo de cada clock nas tabelas do `<revisão>.sta.rpt`: a menor
/// `Restricted Fmax` (`... Model Fmax Summary`) e as menores folgas
/// (`... Model Setup Summary`, `... Model Hold Summary`) entre os cantos,
/// que são as do resumo multicanto do próprio relatório. `targets` dá a
/// frequência de cada clock da placa. `None` se o relatório não tem
/// nenhuma dessas tabelas.
pub(crate) fn timing(rpt: &str, targets: &[(&str, f64)]) -> Option<TimingSummary> {
    #[derive(Clone, Copy)]
    enum Kind {
        Fmax,
        Setup,
        Hold,
    }
    let mut found = false;
    let mut clocks: Vec<ClockTiming> = Vec::new();
    for table in tables(rpt) {
        let (kind, value, clock) = if table.title.ends_with("Model Fmax Summary") {
            (Kind::Fmax, "Restricted Fmax", "Clock Name")
        } else if table.title.ends_with("Model Setup Summary") {
            (Kind::Setup, "Slack", "Clock")
        } else if table.title.ends_with("Model Hold Summary") {
            (Kind::Hold, "Slack", "Clock")
        } else {
            continue;
        };
        let column = |name: &str| table.header.iter().position(|h| *h == name);
        let (Some(value), Some(clock)) = (column(value), column(clock)) else {
            continue;
        };
        found = true;
        for row in &table.rows {
            let (Some(name), Some(number)) =
                (row.get(clock), row.get(value).and_then(|v| decimal(v)))
            else {
                continue;
            };
            let at = match clocks.iter().position(|c| c.clock == *name) {
                Some(at) => at,
                None => {
                    clocks.push(ClockTiming {
                        clock: (*name).to_owned(),
                        target_mhz: targets
                            .iter()
                            .find(|(signal, _)| signal == name)
                            .map(|(_, mhz)| *mhz),
                        fmax_mhz: None,
                        setup_slack_ns: None,
                        hold_slack_ns: None,
                    });
                    clocks.len() - 1
                }
            };
            let entry = &mut clocks[at];
            let slot = match kind {
                Kind::Fmax => &mut entry.fmax_mhz,
                Kind::Setup => &mut entry.setup_slack_ns,
                Kind::Hold => &mut entry.hold_slack_ns,
            };
            *slot = Some(slot.map_or(number, |old| old.min(number)));
        }
    }
    if !found {
        return None;
    }
    let met = clocks.iter().all(|c| {
        c.setup_slack_ns.is_none_or(|s| s >= 0.0) && c.hold_slack_ns.is_none_or(|s| s >= 0.0)
    });
    Some(TimingSummary { clocks, met })
}

/// `187.41 MHz` e `-1.732` como número.
fn decimal(text: &str) -> Option<f64> {
    text.trim().trim_end_matches("MHz").trim().parse().ok()
}

/// Uma tabela do `.rpt`, com as células já sem os espaços.
struct Table<'a> {
    title: &'a str,
    header: Vec<&'a str>,
    rows: Vec<Vec<&'a str>>,
}

/// As tabelas com título e cabeçalho do `.rpt`:
///
/// ```text
/// +--------------------------------------------------+
/// ; Slow 1200mV 85C Model Fmax Summary               ;
/// +------------+-----------------+------------+------+
/// ; Fmax       ; Restricted Fmax ; Clock Name ; Note ;
/// +------------+-----------------+------------+------+
/// ; 187.41 MHz ; 187.41 MHz      ; CLOCK_50   ;      ;
/// +------------+-----------------+------------+------+
/// ```
fn tables(rpt: &str) -> Vec<Table<'_>> {
    let mut out = Vec::new();
    let mut block: Vec<Vec<&str>> = Vec::new();
    for line in rpt.lines().chain(std::iter::once("")) {
        let line = line.trim_end();
        if line.starts_with('+') {
            continue;
        }
        if line.starts_with(';') {
            block.push(cells(line));
            continue;
        }
        let rows = std::mem::take(&mut block);
        if rows.len() >= 2 && rows[0].len() == 1 {
            let mut rows = rows.into_iter();
            let (Some(title), Some(header)) = (rows.next(), rows.next()) else {
                continue;
            };
            out.push(Table {
                title: title[0],
                header,
                rows: rows.collect(),
            });
        }
    }
    out
}

/// `; a ; b ;` como `["a", "b"]`.
fn cells(line: &str) -> Vec<&str> {
    line.trim_start_matches(';')
        .trim_end_matches(';')
        .split(';')
        .map(str::trim)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Um resumo do Fitter no formato do Quartus Prime 25.1, com números
    /// inventados para o teste.
    const FIT_SUMMARY: &str = "Fitter Status : Successful - Thu Oct 08 15:20:11 2026\n\
        Quartus Prime Version : 25.1std.0 Build 1129 10/21/2025 SC Lite Edition\n\
        Revision Name : lace_board_top\n\
        Top-level Entity Name : lace_board_top\n\
        Family : Cyclone IV E\n\
        Device : EP4CE115F29C7\n\
        Timing Models : Final\n\
        Total logic elements : 2,417 / 114,480 ( 2 % )\n\
        \x20   Total combinational functions : 2,305 / 114,480 ( 2 % )\n\
        \x20   Dedicated logic registers : 812 / 114,480 ( < 1 % )\n\
        Total registers : 812\n\
        Total pins : 67 / 529 ( 13 % )\n\
        Total virtual pins : 0\n\
        Total memory bits : 131,072 / 3,981,312 ( 3 % )\n\
        Embedded Multiplier 9-bit elements : 4 / 532 ( < 1 % )\n\
        Total PLLs : 1 / 4 ( 25 % )\n";

    #[test]
    fn the_fitter_summary() {
        let found = resources(FIT_SUMMARY);
        let names: Vec<&str> = found.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "Total logic elements",
                "Total combinational functions",
                "Dedicated logic registers",
                "Total registers",
                "Total pins",
                "Total virtual pins",
                "Total memory bits",
                "Embedded Multiplier 9-bit elements",
                "Total PLLs",
            ]
        );
        assert_eq!(
            found[0],
            ResourceUsage {
                name: "Total logic elements".into(),
                used: 2417,
                available: Some(114480),
                detail: false,
            }
        );
        assert!(found[1].detail);
        assert_eq!((found[3].used, found[3].available), (812, None));
        assert_eq!(found[6].used, 131_072);
    }

    /// As tabelas de tempo no formato do Quartus Prime, em três cantos, com
    /// dois clocks e números inventados para o teste.
    const STA_REPORT: &str = "\
+--------------------------------------------------+
; Slow 1200mV 85C Model Fmax Summary               ;
+------------+-----------------+------------+------+
; Fmax       ; Restricted Fmax ; Clock Name ; Note ;
+------------+-----------------+------------+------+
; 187.41 MHz ; 187.41 MHz      ; CLOCK_50   ;      ;
+------------+-----------------+------------+------+
This panel reports FMAX for every clock in the design.


+----------------------------------------------------------------+
; Slow 1200mV 85C Model Setup Summary                            ;
+---------------------------------------+--------+---------------+
; Clock                                 ; Slack  ; End Point TNS ;
+---------------------------------------+--------+---------------+
; CLOCK_50                              ; -1.732 ; -24.906       ;
; uart_rx:serial|baud_tick             ; -0.958 ; -1.204        ;
+---------------------------------------+--------+---------------+


+---------------------------------------------------------------+
; Slow 1200mV 85C Model Hold Summary                            ;
+---------------------------------------+-------+---------------+
; Clock                                 ; Slack ; End Point TNS ;
+---------------------------------------+-------+---------------+
; CLOCK_50                              ; 0.402 ; 0.000         ;
; uart_rx:serial|baud_tick             ; 0.611 ; 0.000         ;
+---------------------------------------+-------+---------------+


------------------------------------------
; Slow 1200mV 85C Model Recovery Summary ;
------------------------------------------
No paths to report.


+--------------------------------------------------+
; Slow 1200mV 0C Model Fmax Summary                ;
+------------+-----------------+------------+------+
; Fmax       ; Restricted Fmax ; Clock Name ; Note ;
+------------+-----------------+------------+------+
; 196.52 MHz ; 196.52 MHz      ; CLOCK_50   ;      ;
+------------+-----------------+------------+------+


+----------------------------------------------------------------+
; Fast 1200mV 0C Model Setup Summary                             ;
+---------------------------------------+--------+---------------+
; Clock                                 ; Slack  ; End Point TNS ;
+---------------------------------------+--------+---------------+
; uart_rx:serial|baud_tick             ; -0.317 ; -0.317        ;
; CLOCK_50                              ; -0.205 ; -1.330        ;
+---------------------------------------+--------+---------------+


+---------------------------------------------------------------+
; Fast 1200mV 0C Model Hold Summary                             ;
+---------------------------------------+-------+---------------+
; Clock                                 ; Slack ; End Point TNS ;
+---------------------------------------+-------+---------------+
; CLOCK_50                              ; 0.181 ; 0.000         ;
; uart_rx:serial|baud_tick             ; 0.274 ; 0.000         ;
+---------------------------------------+-------+---------------+
";

    #[test]
    fn the_worst_corner_of_each_clock() {
        let timing = timing(STA_REPORT, &[("CLOCK_50", 50.0)]).unwrap();
        assert!(!timing.met);
        assert_eq!(timing.clocks.len(), 2);
        // O pior canto de cada clock: setup -1.732 e hold 0.181 no
        // CLOCK_50; -0.958 e 0.274 no outro.
        let clk = &timing.clocks[0];
        assert_eq!(clk.clock, "CLOCK_50");
        assert_eq!(clk.target_mhz, Some(50.0));
        assert_eq!(clk.fmax_mhz, Some(187.41));
        assert_eq!(clk.setup_slack_ns, Some(-1.732));
        assert_eq!(clk.hold_slack_ns, Some(0.181));
        let state = &timing.clocks[1];
        assert_eq!(state.target_mhz, None);
        assert_eq!(state.fmax_mhz, None);
        assert_eq!(state.setup_slack_ns, Some(-0.958));
        assert_eq!(state.hold_slack_ns, Some(0.274));
    }

    #[test]
    fn a_report_without_the_tables_says_nothing() {
        assert_eq!(timing("Timing Analyzer report\n", &[]), None);
        let positive = STA_REPORT.replace("; -", "; ");
        assert!(timing(&positive, &[]).unwrap().met);
    }
}
