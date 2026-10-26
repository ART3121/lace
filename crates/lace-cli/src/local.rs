//! O despacho do `lace serve` no mesmo processo. Cada comando da CLI que tem
//! método no protocolo monta o pedido ([`Request`]), o [`Server`] o atende
//! como atende o `lace serve` e o `lace mcp`, e a CLI escreve a resposta: o
//! texto de sempre e o relatório do `--json`, que não muda. Não há processo
//! a mais: o despacho é uma chamada de função, e o comando termina com o
//! processo, como sempre.
//!
//! A CLI fica com o que é só dela: os caminhos relativos ao shell, o
//! processador da pasta atual, as perguntas no terminal, o texto e os
//! relatórios. Instalar, atualizar, desinstalar e o `lace learn` chamam as
//! bibliotecas direto, porque têm interface própria no terminal (a TUI, as
//! barras, as tabelas, as perguntas no meio da operação).

use lace_core::Control;
use lace_core::flows::ProgressFn;
use lace_core::protocol::{Request, Response};
use serde_json::Value;

use crate::Cli;
use crate::serve::{Direct, Failure, Server, Sink, Writer};

/// O despacho, com o bundle resolvido como na linha de comando: um
/// `--compiler` ou um `--quartus` que não serve é erro do comando.
pub(crate) struct Local {
    server: Server,
}

impl Local {
    pub(crate) fn new(cli: &Cli) -> Self {
        Local {
            server: Server::new(cli.toolchain.clone(), Writer::silent()).strict(),
        }
    }

    /// Atende um pedido que não mostra nada enquanto roda.
    pub(crate) fn call(&self, request: Request) -> anyhow::Result<Response> {
        self.run(request, &Control::new(), &|_| {})
    }

    /// Atende `request` com a saída das ferramentas no `control` da CLI, que
    /// também cancela (Ctrl+C), e com o que um fluxo diz entre um passo e
    /// outro em `progress`, na ordem em que acontece. O erro sai com o tipo
    /// de antes (o do core, o da CLI), para o código de saída, a dica e o
    /// `--json` não mudarem.
    pub(crate) fn run(
        &self,
        request: Request,
        control: &Control,
        progress: ProgressFn<'_>,
    ) -> anyhow::Result<Response> {
        let cancel = control.cancel_token().clone();
        let sink = Sink::Direct(Direct { control, progress });
        self.server
            .handle_with(&Value::Null, request, cancel, sink)
            .map_err(Failure::into_anyhow)
    }
}

/// O valor de uma resposta, do tipo que o método dá: `answer!(response,
/// Status)` num `project.status`. Outro tipo é um defeito do Lace, e não do
/// pedido.
macro_rules! answer {
    ($response:expr, $variant:ident) => {
        match $response {
            lace_core::protocol::Response::$variant(value) => value,
            _ => return Err($crate::local::unexpected(stringify!($variant))),
        }
    };
}

pub(crate) use answer;

/// O erro de uma resposta que não é do tipo que o método dá.
pub(crate) fn unexpected(variant: &str) -> anyhow::Error {
    anyhow::anyhow!("The Lace dispatch did not answer with {variant}; this is a bug in Lace")
}
