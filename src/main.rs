mod consensus_module;

use consensus_module::{ConsensusModule, Peer};
use std::{sync::mpsc::channel, thread};

const REPLICAS: usize = 5;

fn main() {
    let mut peer_protos = Vec::new();
    let mut receivers = Vec::new();
    for id in 0..=REPLICAS {
        let (tx, rx) = channel();

        peer_protos.push(Peer::new(id, tx));
        receivers.push((id, rx));
    }

    while receivers.len() != 0 {
        if let Some((id, rx)) = receivers.pop() {
            let mut peers = peer_protos.clone();
            peers.pop();

            thread::spawn(move || {
                ConsensusModule::start(id, rx, peers);
            });
        }
    }

    loop {}
}
