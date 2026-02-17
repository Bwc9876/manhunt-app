use std::{collections::HashSet, marker::PhantomData, pin::Pin, sync::Arc};

use anyhow::{Context, anyhow};
use futures::{
    SinkExt, Stream, StreamExt,
    channel::mpsc::{UnboundedReceiver, UnboundedSender},
};
use log::{error, info};
use matchbox_socket::{Error as SocketError, PeerId, PeerState, WebRtcSocket};
use tokio::{
    sync::{Mutex, mpsc},
    task::yield_now,
};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use manhunt_logic::{Transport, TransportMessage, prelude::*};

use crate::{packets::PacketHandler, server};

type QueuePair<T> = (mpsc::Sender<T>, Mutex<mpsc::Receiver<T>>);
type MsgPair = (Option<Uuid>, TransportMessage);
type Queue = QueuePair<MsgPair>;

pub struct MatchboxTransport<S: SocketImpl = WebRtcSocket> {
    my_id: Uuid,
    incoming: Queue,
    all_peers: Mutex<HashSet<Uuid>>,
    msg_sender: UnboundedSender<MatchboxMsgPair>,
    cancel_token: CancellationToken,
    phantom: PhantomData<S>,
}

type LoopFutRes = Result<(), SocketError>;

type MatchboxMsgPair = (PeerId, Box<[u8]>);
type MatchboxSender = UnboundedSender<MatchboxMsgPair>;
type MatchboxReceiver = UnboundedReceiver<MatchboxMsgPair>;
type MatchboxChannel = (MatchboxSender, MatchboxReceiver);

fn map_socket_error(err: SocketError) -> anyhow::Error {
    match err {
        SocketError::ConnectionFailed(e) => anyhow!("Connection to server failed: {e:?}"),
        SocketError::Disconnected(e) => anyhow!("Connection to server lost: {e:?}"),
    }
}

type FutPin<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type MessageLoopFuture = FutPin<LoopFutRes>;
type PeerMsg = (PeerId, PeerState);

pub trait SocketImpl: Unpin + Send + Sync + Sized + Stream<Item = PeerMsg> {
    fn new(room_url: &str) -> (Self, MessageLoopFuture);
    fn get_id(&mut self) -> Option<PeerId>;
    fn take_channel(&mut self) -> MatchboxChannel;
}

impl SocketImpl for WebRtcSocket {
    fn new(room_url: &str) -> (Self, MessageLoopFuture) {
        Self::new_reliable(room_url)
    }

    fn get_id(&mut self) -> Option<PeerId> {
        self.id()
    }

    fn take_channel(&mut self) -> MatchboxChannel {
        self.take_channel(0).expect("Failed to get channel").split()
    }
}

impl<S: SocketImpl + 'static> MatchboxTransport<S> {
    pub async fn new(join_code: &str, is_host: bool) -> Result<Arc<Self>> {
        let ws_url = server::room_url(join_code, is_host);
        let (socket, loop_fut) = S::new(&ws_url);
        Self::from_socket_and_loop_fut(socket, loop_fut).await
    }

    async fn from_socket_and_loop_fut(
        mut socket: S,
        mut loop_fut: MessageLoopFuture,
    ) -> Result<Arc<Self>> {
        let (itx, irx) = mpsc::channel(15);
        let (mtx, mrx) = socket.take_channel();

        let res = loop {
            tokio::select! {
                id = Self::wait_for_id(&mut socket) => {
                    if let Some(id) = id {
                        break Ok(id);
                    }
                },
                res = &mut loop_fut => {
                    break Err(match res {
                        Ok(_) => anyhow!("Transport disconnected unexpectedly"),
                        Err(err) => map_socket_error(err)
                    });
                }
            }
        }
        .context("While trying to join the lobby");

        match res {
            Ok(my_id) => {
                let transport = Arc::new(Self {
                    my_id,
                    incoming: (itx, Mutex::new(irx)),
                    all_peers: Mutex::new(HashSet::with_capacity(5)),
                    msg_sender: mtx.clone(),
                    cancel_token: CancellationToken::new(),
                    phantom: PhantomData,
                });

                tokio::spawn({
                    let transport = transport.clone();
                    async move {
                        transport.main_loop(socket, loop_fut, mrx).await;
                    }
                });

                Ok(transport)
            }
            Err(why) => {
                drop(mrx);
                drop(mtx);
                drop(socket);
                Err(why)
            }
        }
    }

    async fn wait_for_id(socket: &mut S) -> Option<Uuid> {
        if let Some(id) = socket.get_id() {
            Some(id.0)
        } else {
            yield_now().await;
            None
        }
    }

    async fn push_incoming(&self, id: Option<Uuid>, msg: TransportMessage) {
        self.incoming
            .0
            .send((id, msg))
            .await
            .expect("Failed to push to incoming queue");
    }

    async fn main_loop(
        &self,
        mut socket: S,
        loop_fut: MessageLoopFuture,
        mut mrx: UnboundedReceiver<MatchboxMsgPair>,
    ) {
        tokio::pin!(loop_fut);
        let mut packet_handler = PacketHandler::default();

        info!("Starting transport loop");

        let (should_await, msg) = loop {
            tokio::select! {
                biased;

                res = &mut loop_fut => {
                    info!("Transport-initiated disconnect");
                    break (false, match res {
                        Ok(_) => TransportMessage::Disconnected,
                        Err(e) => {
                            let msg = map_socket_error(e).to_string();
                            TransportMessage::Error(msg)
                        }
                    });
                }

                _ = self.cancel_token.cancelled() => {
                    info!("Logic-initiated disconnect");
                    break (true, TransportMessage::Disconnected);
                }

                Some((peer, state)) = socket.next() => {
                    info!("Handling peer {peer}: {state:?}");
                    self.handle_peer(peer, state).await;
                }

                Some(data) = mrx.next() => {
                    info!("Handling new packet from {}", data.0);
                    self.handle_recv(data, &mut packet_handler).await;
                }


            }
        };

        self.push_incoming(Some(self.my_id), msg).await;

        self.msg_sender.close_channel();
        self.incoming.1.lock().await.close();
        drop(mrx);
        drop(socket);
        if should_await {
            if let Err(why) = loop_fut.await {
                error!("Failed to await after disconnect: {why:?}");
            }
        }
        info!("Transport disconnected");
    }

    async fn handle_peer(&self, peer: PeerId, state: PeerState) {
        let mut all_peers = self.all_peers.lock().await;
        let msg = match state {
            PeerState::Connected => {
                all_peers.insert(peer.0);
                TransportMessage::PeerConnect(peer.0)
            }
            PeerState::Disconnected => {
                all_peers.remove(&peer.0);
                TransportMessage::PeerDisconnect(peer.0)
            }
        };
        drop(all_peers);
        self.push_incoming(Some(peer.0), msg).await;
    }

    async fn handle_recv(
        &self,
        (PeerId(peer), packet): MatchboxMsgPair,
        handler: &mut PacketHandler,
    ) {
        match handler.consume_packet(peer, packet.into_vec()) {
            Ok(Some(msg)) => {
                self.push_incoming(Some(peer), msg).await;
            }
            Ok(None) => {
                // Non complete message
            }
            Err(why) => {
                error!("Error receiving message: {why}");
            }
        }
    }

    pub async fn send_transport_message(&self, peer: Option<Uuid>, msg: TransportMessage) {
        let mut tx = self.msg_sender.clone();

        match PacketHandler::message_to_packets(&msg) {
            Ok(packets) => {
                if let Some(peer) = peer {
                    let mut stream = futures::stream::iter(
                        packets
                            .into_iter()
                            .map(|p| Ok((PeerId(peer), p.into_boxed_slice()))),
                    );
                    if let Err(why) = tx.send_all(&mut stream).await {
                        error!("Error sending packet: {why}");
                    }
                } else {
                    let all_peers = self.all_peers.lock().await;
                    for peer in all_peers.iter().copied() {
                        let packets = packets.clone();
                        let mut stream = futures::stream::iter(
                            packets
                                .into_iter()
                                .map(|p| Ok((PeerId(peer), p.into_boxed_slice()))),
                        );
                        if let Err(why) = tx.send_all(&mut stream).await {
                            error!("Error sending packet: {why}");
                        }
                    }
                }
            }
            Err(why) => {
                error!("Error encoding message: {why}");
            }
        }
    }

    pub async fn recv_transport_messages(&self) -> Vec<MsgPair> {
        let mut incoming_rx = self.incoming.1.lock().await;
        let mut buffer = Vec::with_capacity(60);
        incoming_rx.recv_many(&mut buffer, 60).await;
        buffer
    }

    #[cfg(test)]
    pub async fn force_recv_msg(&self) -> MsgPair {
        self.incoming
            .1
            .lock()
            .await
            .recv()
            .await
            .expect("No messages")
    }

    #[cfg(test)]
    pub async fn assert_no_incoming(&self) {
        assert!(self.incoming.1.lock().await.is_empty());
    }

    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}

impl<S: SocketImpl + 'static> Transport for MatchboxTransport<S> {
    fn self_id(&self) -> Uuid {
        self.my_id
    }

    async fn receive_messages(&self) -> impl Iterator<Item = manhunt_logic::MsgPair> {
        self.recv_transport_messages().await.into_iter()
    }

    async fn send_message_single(&self, peer: Uuid, msg: TransportMessage) {
        self.send_transport_message(Some(peer), msg).await;
    }

    async fn send_message(&self, msg: TransportMessage) {
        self.send_transport_message(None, msg).await;
    }

    async fn send_self(&self, msg: TransportMessage) {
        self.push_incoming(Some(self.my_id), msg).await;
    }

    async fn room_joinable(&self, code: &str) -> bool {
        server::room_exists(code).await.unwrap_or(false)
    }

    async fn mark_room_started(&self, code: &str) {
        if let Err(why) = server::mark_room_started(code).await {
            error!("Failed to mark room {code} as started: {why:?}");
        }
    }

    async fn disconnect(&self) {
        self.cancel();
    }

    async fn initialize(code: &str, host: bool) -> Result<Arc<Self>> {
        Self::new(code, host).await
    }
}

#[cfg(test)]
mod tests {

    use futures::{
        channel::{mpsc, oneshot},
        lock::Mutex as FutMutex,
    };
    use manhunt_logic::{GameEvent, LobbyMessage, PlayerProfile};
    use matchbox_socket::SignalingError;

    use super::*;
    use tokio::test;

    use std::{collections::HashMap, sync::Mutex as StdMutex, time::Duration};

    type PeerRx = UnboundedReceiver<PeerMsg>;
    type PeerTx = UnboundedSender<PeerMsg>;
    type IdHandle = Arc<StdMutex<Option<PeerId>>>;

    struct MockSocket {
        peer_recv: PeerRx,
        id: IdHandle,
        channel: Option<MatchboxChannel>,
        cancel: CancellationToken,
    }

    impl MockSocket {
        pub fn new(
            peer_recv: PeerRx,
            channel: MatchboxChannel,
            id: IdHandle,
        ) -> (
            Self,
            MessageLoopFuture,
            oneshot::Sender<LoopFutRes>,
            CancellationToken,
        ) {
            let (stop_tx, stop_rx) = oneshot::channel();
            let cancel = CancellationToken::new();
            let sock = Self {
                peer_recv,
                channel: Some(channel),
                id,
                cancel: cancel.clone(),
            };

            let fut = Box::pin(async move { stop_rx.await.expect("Failed to recv") });

            (sock, fut, stop_tx, cancel)
        }
    }

    impl Drop for MockSocket {
        fn drop(&mut self) {
            self.cancel.cancel();
        }
    }

    impl Stream for MockSocket {
        type Item = (PeerId, PeerState);

        fn poll_next(
            self: Pin<&mut Self>,
            cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            let mut peer_state_rx = Pin::new(&mut self.get_mut().peer_recv);
            peer_state_rx.as_mut().poll_next(cx)
        }
    }

    impl SocketImpl for MockSocket {
        fn new(_room_url: &str) -> (Self, MessageLoopFuture) {
            unreachable!("Tests should use [MatchboxTransport::from_socket_and_loop_fut]")
        }

        fn get_id(&mut self) -> Option<PeerId> {
            *self.id.lock().unwrap()
        }

        fn take_channel(&mut self) -> (MatchboxSender, MatchboxReceiver) {
            self.channel.take().expect("Channel already taken")
        }
    }

    type MatchboxTransport = super::MatchboxTransport<MockSocket>;

    struct WaitingPeer {
        incoming: MatchboxSender,
        outgoing: MatchboxReceiver,
        peer_tx: PeerTx,
        intended_id: PeerId,
        id_handle: IdHandle,
        disconnect: oneshot::Sender<LoopFutRes>,
        client_cancel: CancellationToken,
    }

    #[derive(Default, Debug)]
    struct MockSignaling {
        peers: HashMap<
            PeerId,
            (
                PeerTx,
                oneshot::Sender<LoopFutRes>,
                CancellationToken,
                CancellationToken,
            ),
        >,
        senders: Arc<FutMutex<HashMap<PeerId, MatchboxSender>>>,
    }

    impl MockSignaling {
        fn new() -> Self {
            tokio::time::pause();
            Self::default()
        }

        fn client_connect(
            &self,
            id: Uuid,
        ) -> (
            WaitingPeer,
            FutPin<Result<Arc<MatchboxTransport>, anyhow::Error>>,
        ) {
            let (itx, irx) = mpsc::unbounded();
            let (otx, orx) = mpsc::unbounded();
            let (peer_tx, peer_rx) = mpsc::unbounded();
            let id_handle = Arc::new(StdMutex::new(None));

            let (sock, fut, disconnect, cancel) =
                MockSocket::new(peer_rx, (otx, irx), id_handle.clone());

            let transport_fut = Box::pin(MatchboxTransport::from_socket_and_loop_fut(sock, fut));

            let peer = WaitingPeer {
                incoming: itx,
                outgoing: orx,
                peer_tx,
                intended_id: PeerId(id),
                id_handle,
                disconnect,
                client_cancel: cancel,
            };

            (peer, transport_fut)
        }

        async fn broadcast_peer_join(&mut self, source: PeerId) {
            let (source_sender, _, _, _) = self.peers.get(&source).expect("Source not in peers");
            let mut source_sender = source_sender.clone();
            let peers = self.peers.iter_mut().filter(|(k, _)| **k != source);
            for (id, (peer_tx, _, _, _)) in peers {
                peer_tx
                    .send((source, PeerState::Connected))
                    .await
                    .expect("Failed to send");
                source_sender
                    .send((*id, PeerState::Connected))
                    .await
                    .expect("Failed to send");
            }
        }

        async fn broadcast_peer_leave(&mut self, source: PeerId) {
            let peers = self.peers.iter_mut().filter(|(k, _)| **k != source);
            for (_, (peer_tx, _, _, _)) in peers {
                peer_tx
                    .send((source, PeerState::Disconnected))
                    .await
                    .expect("Failed to send");
            }
        }

        /// Assign an ID to a MockSocket and set it so the future resolves
        async fn assign_id(&mut self, waiting: WaitingPeer) {
            let WaitingPeer {
                id_handle,
                intended_id,
                incoming,
                mut outgoing,
                peer_tx,
                disconnect,
                client_cancel,
            } = waiting;

            let cancel = CancellationToken::new();

            *id_handle.lock().unwrap() = Some(intended_id);
            self.peers.insert(
                intended_id,
                (peer_tx, disconnect, cancel.clone(), client_cancel),
            );
            self.senders.lock().await.insert(intended_id, incoming);
            self.broadcast_peer_join(intended_id).await;

            let senders = self.senders.clone();

            tokio::spawn(async move {
                let id = intended_id;
                loop {
                    tokio::select! {
                        biased;

                        _ = cancel.cancelled() => { break; }

                        Some((peer, packet)) = outgoing.next() => {
                            let mut senders = senders.lock().await;
                            let sender = senders.get_mut(&peer).expect("Failed to find peer");
                            sender.send((id, packet)).await.expect("Failed to send");
                        }
                    }
                }
            });
        }

        async fn disconnect_peer(&mut self, id: Uuid, res: LoopFutRes) {
            let (_, dc, cancel, _) = self.peers.remove(&PeerId(id)).expect("Peer not connected");
            cancel.cancel();
            dc.send(res).expect("Failed to send dc");
            self.broadcast_peer_leave(PeerId(id)).await;
        }

        async fn wait_for_socket_drop(&self, id: Uuid) {
            let cancel = self.peers.get(&PeerId(id)).unwrap().3.clone();
            cancel.cancelled().await;
        }

        async fn wait_for_client_disconnected(&mut self, id: Uuid) {
            self.wait_for_socket_drop(id).await;
            self.disconnect_peer(id, Ok(())).await;
        }

        async fn wait(&self) {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }

        async fn quick_join(&mut self, id: Uuid) -> Arc<MatchboxTransport> {
            let (wait, fut) = self.client_connect(id);
            self.assign_id(wait).await;
            fut.await.expect("Transport init failed")
        }
    }

    const fn id(x: u128) -> Uuid {
        Uuid::from_u128(x)
    }

    #[test]
    async fn test_full_loop() {
        let mut sig = MockSignaling::new();

        let (wait, fut) = sig.client_connect(id(1));

        sig.assign_id(wait).await;

        let transport = fut.await.expect("Tansport failed to initialize");

        assert_eq!(transport.my_id, id(1));

        transport.disconnect().await;

        sig.wait_for_client_disconnected(id(1)).await;
    }

    #[test]
    async fn test_dc_pre_assign() {
        let sig = MockSignaling::new();

        let (wait, fut) = sig.client_connect(id(1));

        wait.disconnect.send(Ok(())).expect("Failed to send");

        let res = fut.await;

        assert!(res.is_err());
        assert!(wait.incoming.is_closed());
        assert!(wait.peer_tx.is_closed());
        assert!(wait.client_cancel.is_cancelled());
    }

    #[test]
    async fn test_err_pre_assign() {
        let sig = MockSignaling::new();

        let (wait, fut) = sig.client_connect(id(1));

        wait.disconnect
            .send(Err(SocketError::Disconnected(
                SignalingError::UnknownFormat,
            )))
            .expect("Failed to send");

        let res = fut.await;

        assert!(res.is_err());
        assert!(wait.incoming.is_closed());
        assert!(wait.peer_tx.is_closed());
        assert!(wait.client_cancel.is_cancelled());
    }

    #[test]
    async fn test_graceful_disconnect() {
        let mut sig = MockSignaling::new();

        let (wait, fut) = sig.client_connect(id(1));

        let can = wait.client_cancel.clone();

        sig.assign_id(wait).await;

        let transport = fut.await.expect("Transport init failed");

        sig.disconnect_peer(id(1), Ok(())).await;

        let (_, disconnected) = transport
            .incoming
            .1
            .lock()
            .await
            .recv()
            .await
            .expect("Transport didnt send error");

        assert!(matches!(disconnected, TransportMessage::Disconnected));

        can.cancelled().await;

        assert!(transport.incoming.0.is_closed());
        assert!(transport.msg_sender.is_closed());
    }

    #[test]
    async fn test_error_handle() {
        let mut sig = MockSignaling::new();

        let (wait, fut) = sig.client_connect(id(1));

        let can = wait.client_cancel.clone();

        sig.assign_id(wait).await;

        let transport = fut.await.expect("Transport init failed");

        sig.disconnect_peer(
            id(1),
            Err(SocketError::Disconnected(SignalingError::UnknownFormat)),
        )
        .await;

        let (_, disconnected) = transport
            .incoming
            .1
            .lock()
            .await
            .recv()
            .await
            .expect("Transport didnt send error");

        assert!(matches!(disconnected, TransportMessage::Error(_)));

        // Wait for the transport to drop the socket
        can.cancelled().await;

        assert!(transport.incoming.0.is_closed());
        assert!(transport.msg_sender.is_closed());
    }

    #[test]
    async fn test_message_passing() {
        let mut sig = MockSignaling::new();

        let t1 = sig.quick_join(id(1)).await;
        let t2 = sig.quick_join(id(2)).await;

        sig.wait().await;

        let (_, msg) = t1.force_recv_msg().await;
        let (_, msg2) = t2.force_recv_msg().await;

        assert_eq!(t1.all_peers.lock().await.len(), 1);
        assert_eq!(t2.all_peers.lock().await.len(), 1);
        assert!(matches!(msg, TransportMessage::PeerConnect(pid) if pid == id(2)));
        assert!(matches!(msg2, TransportMessage::PeerConnect(pid) if pid == id(1)));

        t1.send_transport_message(Some(id(2)), GameEvent::PlayerCaught(id(1)).into())
            .await;

        sig.wait().await;

        let (_, msg) = t2.force_recv_msg().await;

        assert!(
            matches!(msg, TransportMessage::Game(ge) if matches!(*ge, GameEvent::PlayerCaught(i) if i == id(1)))
        );

        t2.send_transport_message(None, LobbyMessage::PlayerSwitch(id(2), true).into())
            .await;

        sig.wait().await;

        let (_, msg) = t1.force_recv_msg().await;

        assert!(
            matches!(msg, TransportMessage::Lobby(lm) if matches!(*lm, LobbyMessage::PlayerSwitch(i, b) if i == id(2) && b))
        );
    }

    #[test]
    async fn test_msg_broadcast() {
        let mut sig = MockSignaling::new();

        let t1 = sig.quick_join(id(1)).await;
        let t2 = sig.quick_join(id(2)).await;
        let t3 = sig.quick_join(id(3)).await;
        let t4 = sig.quick_join(id(4)).await;

        sig.wait().await;

        let ts = [t1, t2, t3, t4];

        for t in ts.iter() {
            assert_eq!(t.all_peers.lock().await.len(), ts.len() - 1);
            // Eat the PeerConnected messages
            for _ in 0..(ts.len() - 1) {
                t.force_recv_msg().await;
            }
        }

        ts[0]
            .send_transport_message(None, GameEvent::PlayerCaught(id(1)).into())
            .await;

        sig.wait().await;

        ts[0].assert_no_incoming().await;

        for t in ts.iter().skip(1) {
            let (pid, msg) = t.force_recv_msg().await;
            assert_eq!(pid, Some(id(1)));
            assert!(
                matches!(msg, TransportMessage::Game(ge) if matches!(*ge, GameEvent::PlayerCaught(i) if i == id(1)))
            );
        }
    }

    #[test]
    async fn test_direct_msg() {
        let mut sig = MockSignaling::new();

        let t1 = sig.quick_join(id(1)).await;
        let t2 = sig.quick_join(id(2)).await;
        let t3 = sig.quick_join(id(3)).await;

        sig.wait().await;

        let ts = [t1, t2, t3];

        for t in ts.iter() {
            assert_eq!(t.all_peers.lock().await.len(), ts.len() - 1);
            // Eat the PeerConnected messages
            for _ in 0..(ts.len() - 1) {
                t.force_recv_msg().await;
            }
        }

        ts[0]
            .send_transport_message(Some(id(2)), GameEvent::PlayerCaught(id(1)).into())
            .await;

        sig.wait().await;

        ts[0].assert_no_incoming().await;
        ts[2].assert_no_incoming().await;

        let (pid, msg) = ts[1].force_recv_msg().await;
        assert_eq!(pid, Some(id(1)));
        assert!(
            matches!(msg, TransportMessage::Game(ge) if matches!(*ge, GameEvent::PlayerCaught(i) if i == id(1)))
        );
    }

    #[test]
    async fn test_multiple_disconnect() {
        let mut sig = MockSignaling::new();

        let t1 = sig.quick_join(id(1)).await;
        let t2 = sig.quick_join(id(2)).await;
        let t3 = sig.quick_join(id(3)).await;

        sig.wait().await;

        let ts = [t1, t2, t3];

        for t in ts.iter() {
            assert_eq!(t.all_peers.lock().await.len(), ts.len() - 1);
            // Eat the PeerConnected messages
            for _ in 0..(ts.len() - 1) {
                t.force_recv_msg().await;
            }
        }

        ts[0].disconnect().await;

        sig.wait_for_client_disconnected(id(1)).await;

        sig.wait().await;

        for t in ts.iter().skip(1) {
            let (_, msg) = t.force_recv_msg().await;

            let all = t.all_peers.lock().await;
            assert!(!all.contains(&id(1)));
            assert!(matches!(msg, TransportMessage::PeerDisconnect(i) if i == id(1)));
        }
    }

    #[test]
    async fn test_big_message() {
        // Just a random string that's bigger than the max packet size
        let pfp = "vbnsj".repeat(65560);
        let pfp2 = pfp.clone();

        let mut sig = MockSignaling::new();

        let t1 = sig.quick_join(id(1)).await;
        let t2 = sig.quick_join(id(2)).await;

        sig.wait().await;

        t1.force_recv_msg().await;
        t2.force_recv_msg().await;

        t1.send_transport_message(
            Some(id(2)),
            LobbyMessage::PlayerSync(
                id(1),
                PlayerProfile {
                    display_name: "asdf".to_string(),
                    pfp_base64: Some(pfp2),
                },
            )
            .into(),
        )
        .await;

        sig.wait().await;

        let (_, msg) = t2.force_recv_msg().await;

        if let TransportMessage::Lobby(le) = msg {
            if let LobbyMessage::PlayerSync(i, p) = *le {
                assert_eq!(i, id(1));
                assert_eq!(p.display_name, "asdf".to_string());
                assert_eq!(p.pfp_base64, Some(pfp));
            } else {
                panic!("Incorrect lobby message");
            }
        } else {
            panic!("Incorrect message");
        }
    }
}
