//! As estatísticas genéricas de síntese: o que o `stat` do Yosys conta no
//! netlist que [`synthesize`](crate::synthesize) gera, para o relatório e
//! para comparar uma síntese com outra ([`crate::history`]).
//!
//! O Lace roda `stat -json -top <topo>` no fim do script da síntese e lê o
//! JSON, e não o texto: as chaves do JSON não mudam com a formatação da
//! tabela. Os números são da seção `design`, que soma os submódulos; num
//! Yosys sem ela, os do módulo de topo.
//!
//! As células são as genéricas do Yosys (`$add`, `$dff`, `$mux`...), depois
//! de `proc` e `opt_clean`: o netlist não é mapeado para uma FPGA, e nada
//! aqui estima LUTs, DSPs, ocupação ou temporização.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Uma contagem do resumo do projeto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum SynthesisMetric {
    /// Módulos no projeto, contando o de topo.
    Modules,
    /// Fios.
    Wires,
    /// Bits de fio.
    WireBits,
    /// Fios com nome do usuário.
    PublicWires,
    /// Bits de fio com nome do usuário.
    PublicWireBits,
    /// Memórias.
    Memories,
    /// Bits de memória.
    MemoryBits,
    /// Processos que sobraram depois do `proc`.
    Processes,
    /// Células, sem contar as instâncias de submódulo.
    Cells,
}

impl SynthesisMetric {
    /// Todas, na ordem do relatório.
    pub const ALL: [SynthesisMetric; 9] = [
        SynthesisMetric::Modules,
        SynthesisMetric::Wires,
        SynthesisMetric::WireBits,
        SynthesisMetric::PublicWires,
        SynthesisMetric::PublicWireBits,
        SynthesisMetric::Memories,
        SynthesisMetric::MemoryBits,
        SynthesisMetric::Processes,
        SynthesisMetric::Cells,
    ];

    /// O nome na tabela do relatório.
    pub fn label(self) -> &'static str {
        match self {
            SynthesisMetric::Modules => "Modules",
            SynthesisMetric::Wires => "Wires",
            SynthesisMetric::WireBits => "Wire bits",
            SynthesisMetric::PublicWires => "Public wires",
            SynthesisMetric::PublicWireBits => "Public wire bits",
            SynthesisMetric::Memories => "Memories",
            SynthesisMetric::MemoryBits => "Memory bits",
            SynthesisMetric::Processes => "Processes",
            SynthesisMetric::Cells => "Cells",
        }
    }

    /// A chave do `stat -json`; `None` para os módulos, que são as entradas
    /// de `modules`.
    fn json_key(self) -> Option<&'static str> {
        Some(match self {
            SynthesisMetric::Modules => return None,
            SynthesisMetric::Wires => "num_wires",
            SynthesisMetric::WireBits => "num_wire_bits",
            SynthesisMetric::PublicWires => "num_pub_wires",
            SynthesisMetric::PublicWireBits => "num_pub_wire_bits",
            SynthesisMetric::Memories => "num_memories",
            SynthesisMetric::MemoryBits => "num_memory_bits",
            SynthesisMetric::Processes => "num_processes",
            SynthesisMetric::Cells => "num_cells",
        })
    }
}

/// Quantas células de um tipo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema, Deserialize)]
#[non_exhaustive]
pub struct CellUsage {
    /// O tipo, como o Yosys o nomeia (`$add`, `$dff`).
    pub cell_type: String,
    /// Quantas.
    pub count: u64,
}

/// O que o `stat` do Yosys contou. Uma contagem que ele não informou fica
/// `null`, e é diferente de zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema, Deserialize)]
#[non_exhaustive]
pub struct SynthesisStatistics {
    /// O Yosys que contou, como ele se identifica (`Yosys 0.69+156 (...)`).
    pub tool: String,
    /// O módulo de topo.
    pub top: String,
    /// Módulos.
    pub modules: Option<u64>,
    /// Fios.
    pub wires: Option<u64>,
    /// Bits de fio.
    pub wire_bits: Option<u64>,
    /// Fios com nome do usuário.
    pub public_wires: Option<u64>,
    /// Bits de fio com nome do usuário.
    pub public_wire_bits: Option<u64>,
    /// Memórias.
    pub memories: Option<u64>,
    /// Bits de memória.
    pub memory_bits: Option<u64>,
    /// Processos.
    pub processes: Option<u64>,
    /// Células, sem as instâncias de submódulo.
    pub cells: Option<u64>,
    /// As células por tipo, da mais usada para a menos usada e, no empate,
    /// pelo nome.
    pub cell_types: Vec<CellUsage>,
}

impl SynthesisStatistics {
    /// O valor de uma contagem.
    pub fn get(&self, metric: SynthesisMetric) -> Option<u64> {
        match metric {
            SynthesisMetric::Modules => self.modules,
            SynthesisMetric::Wires => self.wires,
            SynthesisMetric::WireBits => self.wire_bits,
            SynthesisMetric::PublicWires => self.public_wires,
            SynthesisMetric::PublicWireBits => self.public_wire_bits,
            SynthesisMetric::Memories => self.memories,
            SynthesisMetric::MemoryBits => self.memory_bits,
            SynthesisMetric::Processes => self.processes,
            SynthesisMetric::Cells => self.cells,
        }
    }

    /// O Yosys informou todas as contagens?
    pub fn is_complete(&self) -> bool {
        SynthesisMetric::ALL.iter().all(|&m| self.get(m).is_some())
    }

    fn set(&mut self, metric: SynthesisMetric, value: Option<u64>) {
        let slot = match metric {
            SynthesisMetric::Modules => &mut self.modules,
            SynthesisMetric::Wires => &mut self.wires,
            SynthesisMetric::WireBits => &mut self.wire_bits,
            SynthesisMetric::PublicWires => &mut self.public_wires,
            SynthesisMetric::PublicWireBits => &mut self.public_wire_bits,
            SynthesisMetric::Memories => &mut self.memories,
            SynthesisMetric::MemoryBits => &mut self.memory_bits,
            SynthesisMetric::Processes => &mut self.processes,
            SynthesisMetric::Cells => &mut self.cells,
        };
        *slot = value;
    }

    /// Lê a saída do `stat -json -top <top>`. `None` se o texto não é o
    /// JSON do `stat`.
    pub(crate) fn from_yosys_json(text: &str, top: &str) -> Option<SynthesisStatistics> {
        let json: serde_json::Value = serde_json::from_str(text).ok()?;
        let modules = json.get("modules")?.as_object()?;
        // Os módulos vêm com o `\` dos identificadores do Yosys.
        let names: Vec<&str> = modules
            .keys()
            .map(|k| k.strip_prefix('\\').unwrap_or(k))
            .collect();
        let section = json
            .get("design")
            .or_else(|| modules.get(&format!("\\{top}")))
            .or_else(|| modules.get(top))?
            .as_object()?;
        let tool = json
            .get("creator")
            .and_then(|c| c.as_str())
            .unwrap_or("Yosys")
            .to_owned();
        let mut statistics = SynthesisStatistics {
            tool,
            top: top.to_owned(),
            modules: Some(names.len() as u64),
            wires: None,
            wire_bits: None,
            public_wires: None,
            public_wire_bits: None,
            memories: None,
            memory_bits: None,
            processes: None,
            cells: None,
            cell_types: Vec::new(),
        };
        for metric in SynthesisMetric::ALL {
            if let Some(key) = metric.json_key() {
                statistics.set(metric, section.get(key).and_then(|v| v.as_u64()));
            }
        }
        // `num_cells_by_type` lista também as instâncias de submódulo, que
        // o `num_cells` não conta: elas ficam de fora, e a soma dos tipos
        // bate com as células.
        if let Some(types) = section.get("num_cells_by_type").and_then(|t| t.as_object()) {
            statistics.cell_types = types
                .iter()
                .filter(|(name, _)| !names.contains(&name.strip_prefix('\\').unwrap_or(name)))
                .filter_map(|(name, count)| {
                    Some(CellUsage {
                        cell_type: name.clone(),
                        count: count.as_u64()?,
                    })
                })
                .collect();
            statistics.cell_types.sort_by(|a, b| {
                b.count
                    .cmp(&a.count)
                    .then_with(|| a.cell_type.cmp(&b.cell_type))
            });
        }
        Some(statistics)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A saída do Yosys 0.69 do bundle para um topo com dois submódulos
    /// `leaf` e uma memória.
    const STAT: &str = r#"{
   "creator": "Yosys 0.69+156 (git sha1 9d0c91b23-dirty, Release, Clang /usr/bin/clang++ 21.1.8)",
   "invocation": "stat -json -top top ",
   "modules": {
      "\\top": {
         "num_wires": 9, "num_wire_bits": 81, "num_pub_wires": 6, "num_pub_wire_bits": 37,
         "num_ports": 5, "num_port_bits": 33, "num_memories": 1, "num_memory_bits": 128,
         "num_processes": 0, "num_cells": 4, "num_submodules": 2,
         "num_cells_by_type": { "$add": 1, "$dff": 1, "$memrd": 1, "$memwr_v2": 1, "leaf": 2 }
      },
      "\\leaf": {
         "num_wires": 5, "num_wire_bits": 33, "num_pub_wires": 4, "num_pub_wire_bits": 25,
         "num_ports": 4, "num_port_bits": 25, "num_memories": 0, "num_memory_bits": 0,
         "num_processes": 0, "num_cells": 2, "num_submodules": 0,
         "num_cells_by_type": { "$add": 1, "$dff": 1 }
      }
   },
      "design": {
         "num_wires": 19, "num_wire_bits": 147, "num_pub_wires": 14, "num_pub_wire_bits": 87,
         "num_ports": 13, "num_port_bits": 83, "num_memories": 1, "num_memory_bits": 128,
         "num_processes": 0, "num_cells": 8, "num_submodules": 2,
         "num_cells_by_type": { "$add": 3, "$dff": 3, "$memrd": 1, "$memwr_v2": 1, "leaf": 2 }
      }
}"#;

    #[test]
    fn the_design_section_counts_with_submodules() {
        let s = SynthesisStatistics::from_yosys_json(STAT, "top").unwrap();
        assert!(s.tool.starts_with("Yosys 0.69"));
        assert_eq!(s.modules, Some(2));
        assert_eq!(s.wires, Some(19));
        assert_eq!(s.public_wire_bits, Some(87));
        assert_eq!(s.memory_bits, Some(128));
        assert_eq!(s.processes, Some(0));
        assert_eq!(s.cells, Some(8));
        assert!(s.is_complete());
        // Sem as instâncias de `leaf`, e da mais usada para a menos usada.
        let types: Vec<_> = s
            .cell_types
            .iter()
            .map(|c| (c.cell_type.as_str(), c.count))
            .collect();
        assert_eq!(
            types,
            [("$add", 3), ("$dff", 3), ("$memrd", 1), ("$memwr_v2", 1)]
        );
        assert_eq!(s.cell_types.iter().map(|c| c.count).sum::<u64>(), 8);
    }

    #[test]
    fn without_a_design_section_the_top_module_counts() {
        let json = r#"{"creator": "Yosys", "modules": {"\\soma": {
            "num_wires": 3, "num_cells_by_type": {"$add": 1}}}}"#;
        let s = SynthesisStatistics::from_yosys_json(json, "soma").unwrap();
        assert_eq!(s.modules, Some(1));
        assert_eq!(s.wires, Some(3));
        // Uma contagem que o Yosys não informou não vira zero.
        assert_eq!(s.cells, None);
        assert!(!s.is_complete());
        assert_eq!(s.cell_types.len(), 1);
        assert_eq!(
            SynthesisStatistics::from_yosys_json("not json", "soma"),
            None
        );
        assert_eq!(SynthesisStatistics::from_yosys_json("{}", "soma"), None);
    }
}
