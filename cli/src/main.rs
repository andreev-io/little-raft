extern crate clap;
use simple_raft::replica::{ControlMessage, Message, ReplicaStatus, State, Replica};
use simple_raft::peer::Peer;

use clap::{App, Arg};

use colored::*;
use crossbeam::channel::{Receiver, Sender};
use crossbeam_channel::unbounded;
use std::{collections::BTreeMap, fs, thread, time::Duration};

type PeerSenderProto = (usize, Sender<Message>, Sender<ControlMessage>);
type PeerReceiverProto = (usize, Receiver<Message>, Receiver<ControlMessage>);

// TODO: stdin command input && stdout instructions
//
// TODO: refactoring
//
// TODO: readme

fn main() {
    let matches = App::new("Raft")
        .version("1.0")
        .author("Ilya Andreev <iandre3@illiois.edu>")
        .arg(
            Arg::with_name("num-replicas")
                .long("num-replicas")
                .value_name("NUM_REPLICAS")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("message-drop-percent-probability")
                .long("message-drop-percent-probability")
                .value_name("MESSAGE_DROP_PERCENT_PROBABILITY")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    let num_replicas = matches.value_of("num-replicas").unwrap().parse().unwrap();
    println!("Spinning up {} replicas", num_replicas);
    let (mut receivers, mut transmitters) = (Vec::new(), Vec::new());
    for id in 1..=num_replicas {
        let (tx, rx) = unbounded();
        let (tx_control, rx_control) = unbounded();
        transmitters.push((id, tx, tx_control));
        receivers.push((id, rx, rx_control));
    }

    let drop_prob = matches
        .value_of("message-drop-percent-probability")
        .unwrap()
        .parse()
        .unwrap();
    println!("Dropping messages with {}% probability", drop_prob);
    let (tx_status, rx_status) = unbounded();
    // print!("{esc}[2J{esc}[1;1H", esc = 27 as char);
    start_replica_threads(tx_status, receivers, &transmitters, drop_prob);
    thread::spawn(move || {
        process_status_messages(rx_status, num_replicas);
    });
    process_control_messages(transmitters);
}

// This function blocks forever.
fn process_status_messages(rx_status: Receiver<ReplicaStatus>, num_replicas: usize) {
    let mut statuses = BTreeMap::new();
    loop {
        let status = rx_status.recv().unwrap();
        statuses.insert(status.id, status);
        if statuses.len() == num_replicas {
            let mut ordered_statuses = statuses
                .iter()
                .map(|status| status.1)
                .collect::<Vec<&ReplicaStatus>>();
            ordered_statuses.sort_by_key(|status| status.id);
            let mut output = String::from("");
            for status in ordered_statuses.iter() {
                output.push_str(&print_status(status));
            }

            print!(
                "{esc}[2J{esc}[1;1H{output}",
                esc = 27 as char,
                output = output
            );

            statuses = BTreeMap::new();
        }
    }
}

fn print_status(status: &ReplicaStatus) -> String {
    let mut output = String::from("");
    let state = if status.connected {
        match status.state {
            State::Candidate => format!("{:?}", status.state).blue(),
            State::Leader => format!("{:?}", status.state).cyan(),
            State::Follower => format!("{:?}", status.state).magenta(),
            State::Dead => format!("{:?}", status.state).red(),
        }
    } else {
        format!(
            "Disconnected {}",
            String::from(format!("{:?}", status.state)).to_lowercase()
        )
        .yellow()
    };

    output.push_str(&format!(
        "{}\n",
        format!("Replica {}", status.id).bold().green()
    ));
    output.push_str(&format!(
        "{} with value {} at term {}.\n",
        state.bold(),
        status.value.to_string().bold().cyan(),
        status.term.to_string().bold().cyan()
    ));

    let mut log_string = String::from("");
    for (i, log_entry) in status.log.iter().enumerate() {
        if log_entry.delta == 0 {
            continue;
        }

        let delta_with_sign = if log_entry.delta >= 0 {
            format!("+{}", log_entry.delta)
        } else {
            format!("{}", log_entry.delta)
        };

        if i <= status.last_applied {
            log_string = format!(
                "{} {}",
                log_string,
                format!("(delta {}, term {}) ->", delta_with_sign, log_entry.term).green()
            );
        } else if i <= status.commit_index {
            log_string = format!(
                "{} {}",
                log_string,
                format!("(delta {}, term {}) ->", delta_with_sign, log_entry.term).magenta()
            );
        } else {
            log_string = format!(
                "{} {}",
                log_string,
                format!("(delta {}, term {}) ->", delta_with_sign, log_entry.term).yellow()
            );
        }
    }
    output.push_str(&format!(
        "{} {}\n\n",
        String::from("Log:").bold(),
        log_string
    ));

    output
}

fn start_replica_threads(
    tx_status: Sender<ReplicaStatus>,
    mut receivers: Vec<PeerReceiverProto>,
    transmitters: &Vec<PeerSenderProto>,
    drop_prob: usize,
) {
    while let Some((id, rx, rx_control)) = receivers.pop() {
        let peers = transmitters
            .clone()
            .iter()
            .map(|tuple| Peer::new(tuple.0, tuple.1.clone(), drop_prob))
            .filter(|peer| peer.id != id)
            .collect();

        let t = tx_status.clone();
        thread::spawn(move || {
            Replica::start(id, rx, rx_control, t, peers);
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

// This function blocks forever.
fn process_control_messages(transmitters: Vec<PeerSenderProto>) {
    let mut next_unprocessed_line: usize = 0;
    loop {
        let buffer = match fs::read_to_string("input.txt") {
            Ok(buf) => buf,
            Err(_) => {
                println!("Could not open input.txt");
                continue;
            }
        };

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
                "Connect" => Some(ControlMessage::Connect),
                "Disconnect" => Some(ControlMessage::Disconnect),
                _ => None,
            };

            if let Some(message) = message {
                transmitters[transmitters
                    .binary_search_by_key(&id, |tuple| tuple.0)
                    .unwrap()]
                .2
                .send(message)
                .unwrap();
            } else {
                panic!("Unknown control message. Valid formats: \"5: Down //\" or \"1: Up //\" or \"2: Apply -11//\" or \"2: Connect//\" or \"4: Disconnect//\"");
            }
        }

        thread::sleep(Duration::from_millis(100));
    }
}
