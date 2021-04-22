mod replicas;
mod types;

use crossbeam::channel::{Receiver, Sender};
use crossbeam_channel::unbounded;
use replicas::Replica;
use std::{fs::File, io::prelude::*, thread, time::Duration};
use types::{ControlMessage, Message, Peer};

const REPLICAS: usize = 5;

type PeerSenderProto = (usize, Sender<Message>, Sender<ControlMessage>);
type PeerReceiverProto = (usize, Receiver<Message>, Receiver<ControlMessage>);

fn main() {
    let (mut receivers, mut transmitters) = (Vec::new(), Vec::new());
    for id in 1..=REPLICAS {
        let (tx, rx) = unbounded();
        let (tx_control, rx_control) = unbounded();
        transmitters.push((id, tx, tx_control));
        receivers.push((id, rx, rx_control));
    }

    start_replica_threads(receivers, &transmitters);
    process_control_messages(transmitters);
}

fn start_replica_threads(
    mut receivers: Vec<PeerReceiverProto>,
    transmitters: &Vec<PeerSenderProto>,
) {
    while let Some((id, rx, rx_control)) = receivers.pop() {
        let peers = transmitters
            .clone()
            .iter()
            .map(|tuple| Peer::new(tuple.0, tuple.1.clone()))
            .filter(|peer| peer.id != id)
            .collect();

        thread::spawn(move || {
            Replica::start(id, rx, rx_control, peers);
        });
    }
}

fn parse_control_line(s: &str) -> (usize, String) {
    let entry: Vec<&str> = s.split(":").collect();
    let idx = entry[0].parse::<usize>().expect("non-digit index");
    let mut command = entry[1].to_string();
    command.retain(|c| c != '/' && c != ' ');
    return (idx, command);
}

fn process_control_messages(transmitters: Vec<PeerSenderProto>) {
    let mut next_unprocessed_line: usize = 0;
    loop {
        let mut buffer = String::new();
        File::open("input.txt")
            .unwrap()
            .read_to_string(&mut buffer)
            .unwrap();

        let lines: Vec<&str> = buffer.split("\n").collect();
        if lines.len() >= next_unprocessed_line + 1 && lines[next_unprocessed_line].contains("//") {
            let (id, mut command) = parse_control_line(lines[next_unprocessed_line]);
            next_unprocessed_line += 1;

            let delta = if command.contains("Apply") {
                let delta = command
                    .split("Apply")
                    .nth(1)
                    .unwrap()
                    .parse::<i32>()
                    .expect("unparseable delta");
                command = String::from("Apply");
                delta
            } else {
                0
            };

            let message = match command.as_str() {
                "Up" => Some(ControlMessage::Up),
                "Down" => Some(ControlMessage::Down),
                "Apply" => Some(ControlMessage::Apply(delta)),
                _ => None,
            };

            if let Some(message) = message {
                transmitters[transmitters
                    .binary_search_by_key(&id, |tuple| tuple.0)
                    .unwrap()]
                .2
                .send(message)
                .unwrap();
                println!("Processed line {}", next_unprocessed_line);
            } else {
                println!("Unknown control message. Valid formats: \"5: Down //\" or \"1: Up //\" or \"2: Apply -11//\"");
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}
