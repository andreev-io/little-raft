mod consensus_module;

use consensus_module::{ConsensusModule, Message, Peer};
use crossbeam::channel;
use crossbeam_channel::{unbounded, Select};
use std::{fs::File, io, io::prelude::*, thread, time::Duration};

const REPLICAS: usize = 5;

fn main() {
    let mut peer_protos = Vec::new();
    let mut receivers = Vec::new();
    let mut txs = Vec::new();
    for id in 1..=REPLICAS {
        let (tx, rx) = unbounded();
        let (tx_control, rx_control) = unbounded();
        peer_protos.push(Peer::new(id, tx));
        receivers.push((id, rx, rx_control));
        txs.push((id, tx_control));
    }

    while let Some((id, rx, rx_control)) = receivers.pop() {
        let mut peers = peer_protos.clone();
        peers.retain(|peer| peer.id != id);

        thread::spawn(move || {
            ConsensusModule::start(id, rx, rx_control, peers);
        });
    }

    let mut next_unprocessed_line: usize = 0;
    let mut buffer = String::new();
    loop {
        buffer = String::new();
        File::open("input.txt")
            .unwrap()
            .read_to_string(&mut buffer)
            .unwrap();

        let lines: Vec<&str> = buffer.split("\n").collect();
        if lines.len() >= next_unprocessed_line + 1 && lines[lines.len() - 1].contains("//") {
            let last_line = lines[next_unprocessed_line];
            next_unprocessed_line += 1;
            println!(
                "Processed line {}: \"{}\"",
                next_unprocessed_line, last_line
            );

            let entry: Vec<&str> = last_line.split(":").collect();
            let idx = entry[0].parse::<usize>().expect("non-digit index");
            let mut command = entry[1].to_string();
            command.retain(|c| c != '/' && c != ' ');

            match command.as_str() {
                "Up" => {
                    let tx = &txs[txs
                        .binary_search_by_key(&idx, |entry| entry.0)
                        .expect("could not find peer by index from input file")]
                    .1;
                    tx.send(Message::ControlUp)
                        .expect("failed to send a control message");
                }
                "Down" => {
                    let tx = &txs[txs
                        .binary_search_by_key(&idx, |entry| entry.0)
                        .expect("could not find peer by index from input file")]
                    .1;
                    tx.send(Message::ControlDown)
                        .expect("failed to send a control message");
                }
                _ => {}
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}
