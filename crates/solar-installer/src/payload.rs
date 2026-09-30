//! O índice do payload (`payload/index.json`).

use std::path::Path;

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

use crate::plan::Selection;

/// Nome do diretório do payload, ao lado do executável `install`.
pub const PAYLOAD_DIR: &str = "payload";
/// Nome do índice dentro do payload.
pub const INDEX_FILE: &str = "index.json";
/// Versão do formato do índice.
pub const INDEX_SCHEMA: u32 = 1;

/// O índice: o que o payload instala e em que pedaços.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Index {
    /// [`INDEX_SCHEMA`].
    pub schema: u32,
    /// Versão do Solar.
    pub solar_version: String,
    /// Identificador do bundle de ferramentas.
    pub bundle: String,
    /// Plataforma (`linux-x64`, `darwin-arm64`).
    pub platform: String,
    /// Os componentes que o usuário pode escolher, na ordem de exibição.
    pub components: Vec<ComponentInfo>,
    /// Os pedaços do payload.
    pub chunks: Vec<Chunk>,
}

/// Um componente do bundle, como o instalador o mostra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentInfo {
    /// Nome no manifesto do bundle (`icarus`, `yosys`...).
    pub name: String,
    /// Nome para mostrar.
    pub label: String,
    /// Uma linha sobre o que ele faz.
    pub description: String,
    /// Faz parte do perfil recomendado?
    pub recommended: bool,
    /// Componentes sem os quais este não funciona.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Versão exata no bundle.
    pub version: String,
}

/// Um `.tar.zst` do payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chunk {
    /// Nome do arquivo em `payload/`.
    pub file: String,
    /// Os componentes que usam os arquivos deste pedaço. Vazio: sempre
    /// instalado (o `solar` e o cabeçalho do bundle).
    pub components: Vec<String>,
    /// Bytes extraídos.
    pub size: u64,
    /// Quantos arquivos.
    pub entries: u64,
}

impl Chunk {
    /// Este pedaço vai para a instalação com `selection`?
    pub fn needed_by(&self, selection: &Selection) -> bool {
        self.components.is_empty() || self.components.iter().any(|c| selection.contains(c))
    }
}

impl Index {
    /// Lê `dir/index.json`.
    pub fn load(dir: &Path) -> anyhow::Result<Index> {
        let path = dir.join(INDEX_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("não achei o payload do instalador em {}", path.display()))?;
        let index: Index = serde_json::from_str(&text)
            .with_context(|| format!("índice do payload inválido: {}", path.display()))?;
        if index.schema != INDEX_SCHEMA {
            bail!(
                "índice do payload no formato {}; este instalador entende o {INDEX_SCHEMA}",
                index.schema
            );
        }
        Ok(index)
    }

    /// O componente pelo nome.
    pub fn component(&self, name: &str) -> Option<&ComponentInfo> {
        self.components.iter().find(|c| c.name == name)
    }

    /// Os pedaços que a seleção extrai.
    pub fn chunks_for<'a>(&'a self, selection: &'a Selection) -> impl Iterator<Item = &'a Chunk> {
        self.chunks.iter().filter(|c| c.needed_by(selection))
    }

    /// Bytes que a seleção ocupa depois de instalada.
    pub fn size_of(&self, selection: &Selection) -> u64 {
        self.chunks_for(selection).map(|c| c.size).sum()
    }

    /// Bytes de um componente sozinho (com as bibliotecas que ele divide com
    /// outros), sem o que é sempre instalado.
    pub fn component_size(&self, name: &str) -> u64 {
        self.chunks
            .iter()
            .filter(|c| c.components.iter().any(|n| n == name))
            .map(|c| c.size)
            .sum()
    }
}
