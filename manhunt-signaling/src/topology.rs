use async_trait::async_trait;
use axum::extract::ws::Message;
use futures::StreamExt;
use log::{error, info, warn};
use matchbox_protocol::{JsonPeerEvent, PeerRequest};
use matchbox_signaling::{
    ClientRequestError, NoCallbacks, SignalingTopology, WsStateMeta, common_logic::parse_request,
};

use crate::state::ServerState;

#[derive(Default, Debug)]
pub struct ServerTopology;

#[async_trait]
impl SignalingTopology<NoCallbacks, ServerState> for ServerTopology {
    async fn state_machine(upgrade: WsStateMeta<NoCallbacks, ServerState>) {
        let WsStateMeta {
            peer_id,
            sender,
            mut receiver,
            mut state,
            ..
        } = upgrade;

        let (host, cancel, other_peers) = state.add_peer(peer_id, sender.clone());

        let msg = Message::Text(JsonPeerEvent::NewPeer(peer_id).to_string().into());

        for other_id in other_peers {
            if let Err(why) = state.try_send(other_id, msg.clone()) {
                error!("Failed to publish new peer event to {other_id}: {why:?}");
            }
        }

        loop {
            let next_msg = tokio::select! {
                biased;

                _ = cancel.cancelled() => {
                    info!("Disconnecting {peer_id} due to host disconnect");
                    break;
                }

                next = receiver.next() => {
                    if let Some(next) = next {
                        parse_request(next)
                    } else {
                        info!("Peer {peer_id} has disconnected");
                        break;
                    }
                }
            };

            let req = match next_msg {
                Ok(req) => req,
                Err(e) => match e {
                    ClientRequestError::Axum(e) => {
                        warn!("Peer {peer_id} encountered Axum error: {e:?}. Disconnecting...");
                        break;
                    }
                    ClientRequestError::Close => {
                        info!("Peer {peer_id} closed connection");
                        break;
                    }
                    ClientRequestError::Json(_) | ClientRequestError::UnsupportedType(_) => {
                        error!("Error parsing request from {peer_id}: {e:?}");
                        continue; // Recoverable, although may mean bad state?
                    }
                },
            };

            if let PeerRequest::Signal { receiver, data } = req {
                let msg = Message::Text(
                    JsonPeerEvent::Signal {
                        sender: peer_id,
                        data,
                    }
                    .to_string()
                    .into(),
                );
                if let Err(why) = state.try_send(receiver, msg) {
                    error!("Error sending signaling message from {peer_id} to {receiver}: {why:?}");
                }
            } // Other variant, PeerRequest::KeepAlive is just for a heartbeat, do nothing
        }

        let msg = Message::Text(JsonPeerEvent::PeerLeft(peer_id).to_string().into());
        if let Some(other_peers) = state.remove_peer(peer_id, host) {
            for other_id in other_peers {
                if let Err(why) = state.try_send(other_id, msg.clone()) {
                    warn!("Failed to alert {other_id} that {peer_id} has disconnected: {why:?}");
                }
            }
        } else {
            warn!("Trying to remove peer {peer_id}, which doesn't exist?");
        }
    }
}
