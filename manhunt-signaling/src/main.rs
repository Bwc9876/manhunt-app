mod state;
mod topology;

use axum::{
    extract::Path,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
};
use log::{debug, info};
use matchbox_signaling::SignalingServerBuilder;

use anyhow::Context;
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    result::Result as StdResult,
};

use state::ServerState;
use topology::ServerTopology;

type Result<T = (), E = anyhow::Error> = StdResult<T, E>;

#[tokio::main]
async fn main() -> Result {
    colog::init();

    let args = std::env::args().collect::<Vec<_>>();
    let socket_addr = args
        .get(1)
        .map(|raw_binding| raw_binding.parse::<SocketAddr>())
        .transpose()
        .context("Invalid socket addr passed")?
        .unwrap_or(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 3536));

    let mut state = ServerState::default();

    let server = SignalingServerBuilder::new(socket_addr, ServerTopology, state.clone())
        .on_connection_request({
            let mut state = state.clone();
            move |connection| {
                info!("{} is requesting to connect", connection.origin);
                debug!("Connection meta: {connection:?}");

                let err = if let Some(room_code) = connection.path.clone() {
                    let create = connection.query_params.contains_key("create");
                    match state.handle_room(create, connection.origin, room_code) {
                        Ok(_) => None,
                        Err(err) => Some(err.into()),
                    }
                } else {
                    Some(StatusCode::BAD_REQUEST)
                };

                if let Some(status) = err {
                    Err(status.into_response())
                } else {
                    Ok(true)
                }
            }
        })
        .mutate_router({
            let state = state.clone();
            move |router| {
                let mut state2 = state.clone();
                let state3 = state.clone();
                router
                    .route(
                        "/room_exists/{id}",
                        get(move |Path(room_id): Path<String>| async move {
                            if state.room_is_open(&room_id) {
                                StatusCode::OK
                            } else {
                                StatusCode::NOT_FOUND
                            }
                        }),
                    )
                    .route(
                        "/mark_started/{id}",
                        post(move |Path(room_id): Path<String>| async move {
                            state2.mark_started(&room_id);
                            StatusCode::OK
                        }),
                    )
                    .route(
                        "/gen_code",
                        get(move || async move {
                            state3
                                .generate_room_code()
                                .map_err(|_| StatusCode::CONFLICT)
                        }),
                    )
            }
        })
        .on_id_assignment({
            move |(socket, id)| {
                info!("Assigning id {id} to {socket}");
                state.assign_peer_id(socket, id);
            }
        })
        .build();

    info!(
        "Starting manhunt signaling server {}",
        env!("CARGO_PKG_VERSION")
    );

    server.serve().await.context("Error while running server")
}
