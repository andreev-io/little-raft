extern crate clap;
use simple_raft::cluster::Cluster;
use simple_raft::message::Message;
use simple_raft::replica::Replica;
use simple_raft::state_machine::StateMachine;
use std::collections::BTreeMap;
use std::sync::mpsc::channel;
use std::{fs, sync::mpsc, thread};

#[derive(Clone, Copy, PartialEq, Debug)]
struct CountingMachine {
    value: i32,
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct ALU {
    delta: i32,
}

const HEARTBEAT_TIMEOUT: u64 = 1000;

const ELECTION_MIN_TIMEOUT: u64 = 2500;
const ELECTION_MAX_TIMEOUT: u64 = 3500;

impl StateMachine<ALU> for CountingMachine {
    fn new() -> CountingMachine {
        return CountingMachine { value: 0 };
    }

    fn apply_action(&mut self, action: ALU) {
        self.value += action.delta;
    }
}

#[derive(Debug)]
struct MyCluster {
    self_id: usize,
    receiver: mpsc::Receiver<Message<ALU>>,
    transmitters: BTreeMap<usize, mpsc::Sender<Message<ALU>>>,
    tasks: mpsc::Receiver<ALU>,
}

impl Cluster<ALU> for MyCluster {
    fn send(&self, to_id: usize, message: Message<ALU>) {
        if let Some(transmitter) = self.transmitters.get(&to_id) {
            match transmitter.send(message) {
                Ok(_) => {}
                Err(t) => {
                    println!("{:?}", t);
                }
            }
        }
    }

    fn receive_timeout(&self, timeout: std::time::Duration) -> Option<Message<ALU>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(t) => Some(t),
            Err(_) => None,
        }
    }

    fn get_action(&self) -> Option<ALU> {
        match self.tasks.try_recv() {
            Ok(t) => Some(t),
            Err(_) => None,
        }
    }
}

fn main() {
    let mut transmitters = BTreeMap::new();
    let mut receivers = BTreeMap::new();
    for i in 0..=2 {
        let (tx, rx) = channel::<Message<ALU>>();
        transmitters.insert(i, tx);
        receivers.insert(i, rx);
    }

    let mut clusters = BTreeMap::new();
    let mut task_transmitters = BTreeMap::new();
    for i in 0..=2 {
        let (task_tx, task_rx) = channel::<ALU>();
        task_transmitters.insert(i, task_tx);
        clusters.insert(
            i,
            MyCluster {
                receiver: receivers.remove(&i).unwrap(),
                transmitters: transmitters.clone(),
                self_id: i,
                tasks: task_rx,
            },
        );
    }

    for i in (0..clusters.len()).rev() {
        let cluster = clusters.remove(&i).unwrap();
        let peer_ids = transmitters.keys().cloned().filter(|id| id != &i).collect();
        thread::spawn(move || {
            let mut r = Replica::new(
                i,
                peer_ids,
                Box::new(cluster),
                Box::new(CountingMachine { value: 0 }),
                ALU { delta: 0 },
                std::time::Duration::from_millis(HEARTBEAT_TIMEOUT),
            );
            r.start(ELECTION_MIN_TIMEOUT, ELECTION_MAX_TIMEOUT);
        });
    }

    process_control_messages(task_transmitters);
}

fn parse_control_line(s: &str) -> (usize, String) {
    let entry: Vec<&str> = s.split(":").collect();
    let idx = entry[0].parse::<usize>().expect("non-digit index");
    let mut command = entry[1].to_string();
    command.retain(|c| c != '/' && c != ' ');
    return (idx, command);
}

// This function blocks forever.
fn process_control_messages(transmitters: BTreeMap<usize, mpsc::Sender<ALU>>) {
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

            println!("Action for {:?} with delta {:?}", id, delta);
            let message = match command.as_str() {
                "Apply" => {
                    transmitters.get(&id).unwrap().send(ALU { delta: delta });
                }
                _ => {}
            };
        }

        thread::sleep(std::time::Duration::from_millis(100));
    }
}
