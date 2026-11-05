//! A placa virtual: o `vboard.run` do servidor da AURORA, que compila o
//! projeto com os periféricos da placa e o roda até a interface pedir para
//! parar. Não passa pelas operações (`jobs.rs`): a placa fica ligada
//! enquanto o usuário compila e simula, e a vaga única delas ficaria presa.
//!
//! As notificações do pedido chegam à interface por um `Channel`, com o
//! `type` igual ao método (`events`, `vboard_ready`, `vboard_state`,
//! `vboard_frame`, `vboard_output`), entre `started` (com o id do pedido,
//! que as entradas citam) e `finished` (o `BoardRun`) ou `failed`. As
//! entradas vão por `aurora_call` (`vboard.input`), e parar é o
//! `$/cancel` do pedido.

use serde_json::{Value, json};
use tauri::AppHandle;
use tauri::ipc::Channel;

use crate::error::IpcResult;
use crate::state::blocking;

/// Liga a placa virtual do projeto aberto. Volta logo; a sessão segue numa
/// thread até o fim do pedido.
#[tauri::command]
pub async fn vboard_start(
    app: AppHandle,
    board: Option<String>,
    limit: Option<f64>,
    fps: Option<f64>,
    channel: Channel<Value>,
) -> IpcResult<()> {
    blocking(app, move |app, state| {
        let params = crate::server::with_project(
            state,
            "vboard.run",
            Some(json!({ "board": board, "limit": limit, "fps": fps })),
        )?;
        let app = app.clone();
        std::thread::Builder::new()
            .name("vboard".into())
            .spawn(move || {
                use tauri::Manager;
                let state = app.state::<crate::state::AppState>();
                let notifications = channel.clone();
                let started = channel.clone();
                let result = state.server.call_with(
                    &state.settings.get(),
                    "vboard.run",
                    params,
                    Box::new(move |method, mut params| {
                        if let Some(fields) = params.as_object_mut() {
                            fields.insert("type".to_owned(), Value::String(method.to_owned()));
                            let _ = notifications.send(params);
                        }
                    }),
                    move |id| {
                        let _ = started.send(json!({ "type": "started", "run": id }));
                    },
                );
                let _ = channel.send(match result {
                    Ok(result) => json!({ "type": "finished", "result": result }),
                    Err(error) => json!({ "type": "failed", "error": error }),
                });
            })
            .map(drop)
            .map_err(|error| {
                crate::error::IpcError::new(
                    crate::error::codes::INTERNAL,
                    format!("Could not start the virtual board thread: {error}"),
                )
            })
    })
    .await
}

/// Desliga a placa virtual do pedido `run` (`$/cancel`); a resposta dele
/// chega pelo canal de [`vboard_start`].
#[tauri::command]
pub async fn vboard_stop(app: AppHandle, run: u64) -> IpcResult<()> {
    blocking(app, move |_, state| {
        state.server.cancel(run);
        Ok(())
    })
    .await
}
