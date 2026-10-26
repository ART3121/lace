//! Comandos do bundle: o equivalente do `lace tools`, e instalar
//! componentes e atualizar o Lace.
//!
//! Instalar e atualizar são pedidos ao servidor do Lace (`install.add`,
//! `update.apply`), que roda a biblioteca do instalador com a regra da CLI.
//! Rodam como operação (`jobs.rs`): o progresso chega à interface como
//! `install` (o evento do instalador) e, para o console, como `cli_output`
//! com a linha que a CLI escreveria; o resultado final tem a forma de
//! sempre (`{flow, succeeded, cancelled, exit_code, result}`), com o código
//! de saída que a CLI daria.

use serde_json::{Value, json};
use tauri::AppHandle;
use tauri::ipc::Channel;

use crate::error::{IpcResult, codes};
use crate::jobs::{self, JobMessage, Operation};
use crate::state::blocking;
use crate::toolchain::{self, ToolchainInfo};

/// O bundle, as ferramentas e o compilador do Verilator.
#[tauri::command]
pub async fn toolchain_info(app: AppHandle) -> IpcResult<ToolchainInfo> {
    blocking(app, |_, state| Ok(toolchain::info(&state.settings.get()))).await
}

/// `lace update --check`: o Lace, o bundle e cada aplicativo comparados com
/// a última release e com o upstream (o `update.check` do servidor). Precisa
/// de rede; cada consulta tem prazo. `prefix` vem preenchido quando o
/// servidor é o `lace` de uma instalação do instalador: é por ele que a
/// interface decide o botão de atualizar.
#[tauri::command]
pub async fn lace_update_check(app: AppHandle) -> IpcResult<Value> {
    blocking(app, |_, state| {
        state
            .server
            .call(&state.settings.get(), "update.check", Value::Null)
    })
    .await
}

/// `lace install <componentes>`: instala aplicativos do bundle na
/// instalação do Lace, sem reinstalar (o `install.add` do servidor, que
/// recusa um nome que não é do bundle antes de baixar). Cancelar vale até
/// o último pedaço entrar, e o que entrou sai.
#[tauri::command]
pub async fn lace_install(
    app: AppHandle,
    components: Vec<String>,
    channel: Channel<Value>,
) -> IpcResult<u32> {
    let operation = Operation {
        command: format!("lace install {} --json", components.join(" ")),
        method: "install.add",
        params: json!({ "components": components }),
    };
    jobs::start(
        &app,
        operation,
        channel,
        |_| {},
        |reply| installer_outcome("install", reply),
    )
}

/// `lace update --yes`: troca a instalação pela da última release do Lace,
/// com o bundle e os mesmos componentes (o `update.apply` do servidor). Vai
/// por componentes quando dá: só os pedaços com arquivos que mudaram são
/// baixados e trocados, também no Windows, e o resultado diz o que mudou
/// (`method: "components"`, `changes`). Sem isso, no Linux e no macOS o
/// instalador da release roda e termina junto; no Windows, o assistente de
/// instalação abre e termina depois que o Studio sai (`action` no fim:
/// `updated`, `wizard_opened` ou `up_to_date`).
///
/// Quem confirma é a interface, antes de chamar. Cancelar vale até a troca
/// dos arquivos; durante a troca o pedido é ignorado, e uma troca
/// interrompida é desfeita na próxima operação. Atualizado, o servidor sai
/// (o binário dele foi trocado) e a interface reinicia o Studio
/// ([`crate::exit`]). Com o assistente do Windows, ela fecha o Studio para
/// ele trocar os arquivos, e o assistente abre o Studio quando termina
/// (`reopen_studio`).
#[tauri::command]
pub async fn lace_update(app: AppHandle, channel: Channel<Value>) -> IpcResult<u32> {
    let command = if cfg!(windows) {
        "lace update --yes --json --reopen-studio"
    } else {
        "lace update --yes --json"
    };
    let operation = Operation {
        command: command.to_owned(),
        method: "update.apply",
        params: json!({ "reopen_studio": cfg!(windows) }),
    };
    jobs::start(
        &app,
        operation,
        channel,
        |_| {},
        |reply| installer_outcome("update", reply),
    )
}

/// O resultado de uma instalação ou atualização como a interface o lê:
/// `{flow, succeeded, cancelled, exit_code, result}`, onde `result` é o
/// relatório, ou `{error}` quando falhou, com o código de saída que a CLI
/// daria (0, 2 ou 130).
fn installer_outcome(flow: &str, reply: IpcResult<Value>) -> JobMessage {
    let outcome = match reply {
        Ok(result) => json!({
            "flow": flow,
            "succeeded": true,
            "cancelled": false,
            "exit_code": 0,
            "result": result,
        }),
        Err(error) if error.code == codes::CANCELLED => json!({
            "flow": flow,
            "succeeded": false,
            "cancelled": true,
            "exit_code": 130,
            "result": Value::Null,
        }),
        Err(error) => json!({
            "flow": flow,
            "succeeded": false,
            "cancelled": false,
            "exit_code": 2,
            "result": { "error": { "code": error.code, "message": error.message, "hint": Value::Null } },
        }),
    };
    JobMessage::Finished { outcome }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::IpcError;

    #[test]
    fn installer_outcomes_keep_the_exit_codes_of_the_cli() {
        let JobMessage::Finished { outcome } = installer_outcome("install", Ok(json!({}))) else {
            panic!("finished");
        };
        assert_eq!(outcome["exit_code"], 0);
        let cancelled = Err(IpcError::new(codes::CANCELLED, "Cancelled"));
        let JobMessage::Finished { outcome } = installer_outcome("update", cancelled) else {
            panic!("finished");
        };
        assert_eq!(
            (outcome["cancelled"].clone(), outcome["exit_code"].clone()),
            (json!(true), json!(130))
        );
        let failed = Err(IpcError::new("network", "No network"));
        let JobMessage::Finished { outcome } = installer_outcome("update", failed) else {
            panic!("finished");
        };
        assert_eq!(outcome["exit_code"], 2);
        assert_eq!(outcome["result"]["error"]["code"], "network");
    }
}
