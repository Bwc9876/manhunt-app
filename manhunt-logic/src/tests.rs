use std::{collections::HashMap, sync::Arc};

use tokio::{
    sync::{Mutex, mpsc},
    task::yield_now,
};
use uuid::Uuid;

use crate::{
    MsgPair, StateUpdateSender, Transport, TransportMessage,
    location::{Location, LocationService},
    prelude::*,
};

type GameEventRx = mpsc::Receiver<MsgPair>;
type GameEventTx = mpsc::Sender<MsgPair>;

pub struct MockTransport {
    id: Uuid,
    rx: Mutex<GameEventRx>,
    txs: HashMap<Uuid, GameEventTx>,
}

impl MockTransport {
    pub fn create_mesh(players: u32) -> (Vec<Uuid>, Vec<Self>) {
        let uuids = (0..players)
            .map(|_| uuid::Uuid::new_v4())
            .collect::<Vec<_>>();
        let channels = (0..players)
            .map(|_| tokio::sync::mpsc::channel(20))
            .collect::<Vec<_>>();
        let txs = channels
            .iter()
            .enumerate()
            .map(|(i, (tx, _))| (uuids[i], tx.clone()))
            .collect::<HashMap<_, _>>();

        let transports = channels
            .into_iter()
            .enumerate()
            .map(|(i, (_tx, rx))| Self::new(uuids[i], rx, txs.clone()))
            .collect::<Vec<_>>();

        (uuids, transports)
    }

    pub async fn wait_for_queue_empty(&self) {
        // println!("Waiting for {} queue to empty", self.id);
        loop {
            let all_empty = self
                .txs
                .values()
                .all(|tx| tx.is_closed() || tx.capacity() == tx.max_capacity());

            if all_empty {
                break;
            } else {
                yield_now().await;
            }
        }
    }

    pub async fn fake_join(&self) {
        self.send_message(TransportMessage::PeerConnect(self.id))
            .await;
    }

    pub fn is_disconnected(&self) -> bool {
        self.txs[&self.id].is_closed()
    }

    fn new(id: Uuid, rx: GameEventRx, txs: HashMap<Uuid, GameEventTx>) -> Self {
        Self {
            id,
            rx: Mutex::new(rx),
            txs,
        }
    }
}

impl Transport for MockTransport {
    async fn initialize(_code: &str, _host: bool) -> Result<Arc<Self>> {
        let (_, rx) = mpsc::channel(5);
        Ok(Arc::new(Self {
            id: Uuid::default(),
            rx: Mutex::new(rx),
            txs: HashMap::default(),
        }))
    }

    async fn disconnect(&self) {
        self.send_message(TransportMessage::PeerDisconnect(self.id))
            .await;
        let mut rx = self.rx.lock().await;
        rx.close();
    }

    async fn receive_messages(&self) -> impl Iterator<Item = MsgPair> {
        let mut rx = self.rx.lock().await;
        let mut buf = Vec::with_capacity(20);
        rx.recv_many(&mut buf, 20).await;
        buf.into_iter()
    }

    async fn send_message(&self, msg: TransportMessage) {
        for (_id, tx) in self.txs.iter().filter(|(id, _)| **id != self.id) {
            tx.send((Some(self.id), msg.clone())).await.ok();
        }
    }

    async fn send_message_single(&self, peer: Uuid, msg: TransportMessage) {
        if let Some(tx) = self.txs.get(&peer) {
            tx.send((Some(self.id), msg)).await.ok();
        }
    }

    async fn send_self(&self, msg: TransportMessage) {
        self.txs[&self.id].send((Some(self.id), msg)).await.ok();
    }

    fn self_id(&self) -> Uuid {
        self.id
    }
}

pub struct MockLocation;

impl LocationService for MockLocation {
    fn get_loc(&self) -> Option<Location> {
        Some(crate::location::Location {
            lat: 0.0,
            long: 0.0,
            heading: None,
        })
    }
}

pub struct DummySender;

impl StateUpdateSender for DummySender {
    fn send_update(&self) {}
}
