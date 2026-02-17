use std::collections::HashMap;

use anyhow::{anyhow, bail};
use manhunt_logic::{TransportMessage, prelude::*};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

type PacketEncoded = Vec<u8>;
type PacketSet = Vec<Vec<u8>>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Packet {
    remaining_packets: SeqHeader,
    data: Vec<u8>,
}

type SeqHeader = u64;
const SEQ_HEADER_SIZE: usize = size_of::<SeqHeader>();

const MATCHBOX_MAX_SIZE: usize = 65535;
const PACKET_SIZE: usize = MATCHBOX_MAX_SIZE - SEQ_HEADER_SIZE;
const MAX_NUM_PACKETS: u64 = u64::MAX - 1;

impl Packet {
    pub fn from_raw_bytes(mut bytes: PacketEncoded) -> Result<Self> {
        // First [SEQ_HEADER_SIZE] bytes are our sequence header, in little endian.
        if bytes.len() > SEQ_HEADER_SIZE {
            let rest = bytes.split_off(SEQ_HEADER_SIZE);
            let header = bytes;
            let header = header
                .try_into()
                .map_err(|_| anyhow!("Couldn't parse sequence header"))?;
            let remaining_packets = SeqHeader::from_le_bytes(header);
            // Remaining bytes are the data
            Ok(Self {
                remaining_packets,
                data: rest,
            })
        } else {
            bail!("Incoming packet is not long enough");
        }
    }

    pub fn into_bytes(self) -> PacketEncoded {
        let header_encoded = self.remaining_packets.to_le_bytes();
        header_encoded
            .into_iter()
            .chain(self.data)
            .collect::<Vec<_>>()
    }

    fn packets_needed(len: u64) -> Result<SeqHeader> {
        if len >= MAX_NUM_PACKETS.saturating_mul(PACKET_SIZE as u64) {
            bail!("Message is too long, refusing to send");
        } else if len == 0 {
            bail!("Message is empty");
        } else {
            Ok(len.div_ceil(PACKET_SIZE as u64))
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PacketHandler {
    partials: HashMap<Uuid, PacketSet>,
}

impl PacketHandler {
    fn message_to_bytes(msg: &TransportMessage) -> Result<Vec<u8>> {
        rmp_serde::to_vec(&msg).context("Failed to serialize message")
    }

    fn message_from_bytes(msg: &[u8]) -> Result<TransportMessage> {
        rmp_serde::from_slice(msg).context("Failed to deserialize message")
    }

    pub fn message_to_packets(msg: &TransportMessage) -> Result<PacketSet> {
        let mut bytes = Self::message_to_bytes(msg)?;
        let needed_packets = Packet::packets_needed(bytes.len() as u64)?;
        let mut packets = Vec::with_capacity(needed_packets as usize);
        for i in 1..=needed_packets {
            let remaining_packets = needed_packets - i;
            let mut data = bytes.split_off(bytes.len().min(PACKET_SIZE));
            std::mem::swap(&mut data, &mut bytes);
            packets.push(
                Packet {
                    remaining_packets,
                    data,
                }
                .into_bytes(),
            );
        }

        if !bytes.is_empty() {
            bail!("Bytes not emptied?");
        }

        Ok(packets)
    }

    fn decode_packet_set(set: PacketSet) -> Result<TransportMessage> {
        let combined_bytes = set.into_iter().flatten().collect::<Vec<_>>();
        Self::message_from_bytes(&combined_bytes)
    }

    pub fn consume_packet(
        &mut self,
        peer: Uuid,
        bytes: PacketEncoded,
    ) -> Result<Option<TransportMessage>> {
        match Packet::from_raw_bytes(bytes).context("Failed to decode packet") {
            Ok(Packet {
                remaining_packets,
                data,
            }) => {
                if remaining_packets == 0 {
                    let res = if let Some(mut partial) = self.partials.remove(&peer) {
                        partial.push(data);
                        Self::decode_packet_set(partial)
                    } else {
                        Self::message_from_bytes(&data)
                    };

                    Some(res).transpose()
                } else {
                    let partial = self
                        .partials
                        .entry(peer)
                        .or_insert_with(|| Vec::with_capacity(remaining_packets as usize + 1));
                    partial.push(data);
                    Ok(None)
                }
            }
            Err(why) => {
                // Remove current partial message if we received an invalid packet as the entire
                // sequence will now be wrong.
                self.partials.remove(&peer);
                Err(why)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packets_needed() {
        assert_eq!(Packet::packets_needed(5).unwrap(), 1, "5 bytes, one packet");
        assert_eq!(
            Packet::packets_needed((MATCHBOX_MAX_SIZE + 12) as u64).unwrap(),
            2,
            "MAX + 12 bytes, two packets"
        );
        assert!(
            Packet::packets_needed(0).is_err(),
            "Empty packets disallowed"
        );
        assert!(
            Packet::packets_needed(u64::MAX).is_err(),
            "Too many packets disallowed"
        );
    }

    #[test]
    fn test_basic_packet_handling() {
        let mut handler = PacketHandler::default();

        let msg = TransportMessage::Disconnected;
        let data =
            PacketHandler::message_to_packets(&msg).expect("Failed to make message into bytes");

        assert_eq!(data.len(), 1);

        let data = data.into_iter().next().unwrap();

        let decoded = handler
            .consume_packet(Uuid::default(), data)
            .expect("Failed to load message from bytes")
            .expect("Message not complete despite being less than PACKET_SIZE");

        assert!(
            matches!(decoded, TransportMessage::Disconnected),
            "Transport message does not match input"
        );
    }

    #[test]
    fn test_multipart() {
        // Adding random amount to make sure we account for remainders, etc.
        let really_big_string = "a".repeat(MATCHBOX_MAX_SIZE * 5 + 35);
        let really_big_message = TransportMessage::Error(really_big_string.clone());

        let packets =
            PacketHandler::message_to_packets(&really_big_message).expect("Failed to encode");

        assert!(packets.len() > 1, "Saving in one packet");

        let mut handler = PacketHandler::default();
        let mut res = None;

        for pack in packets {
            assert!(
                pack.len() <= MATCHBOX_MAX_SIZE,
                "Packets aren't small enough, {} > {}",
                pack.len(),
                MATCHBOX_MAX_SIZE
            );

            res = handler
                .consume_packet(Uuid::default(), pack)
                .expect("Failed to decode");
        }

        if let Some(TransportMessage::Error(s)) = res {
            assert_eq!(s, really_big_string, "internal strings aren't equal");
        } else {
            panic!("Decoded is the wrong type or wasn't completed");
        }
    }
}
