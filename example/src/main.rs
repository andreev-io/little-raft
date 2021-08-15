use little_raft::{
    cluster::Cluster, message::Message, replica::Replica, state_machine::StateMachine,
};
use std::{
    collections::BTreeMap,
    fs,
    sync::mpsc::{channel, Receiver, Sender},
    thread,
};

const HEARTBEAT_TIMEOUT: u64 = 1000;
const ELECTION_MIN_TIMEOUT: u64 = 2500;
const ELECTION_MAX_TIMEOUT: u64 = 3500;

#[derive(Clone, Copy)]
struct MathAction {
    delta: i32,
}

struct Calculator {
    value: i32,
}

impl StateMachine<MathAction> for Calculator {
    fn apply_action(&mut self, action: MathAction) {
        self.value += action.delta;
    }
}

struct MyCluster {
    receiver: Receiver<Message<MathAction>>,
    transmitters: BTreeMap<usize, Sender<Message<MathAction>>>,
    tasks: Receiver<MathAction>,
}

impl Cluster<MathAction> for MyCluster {
    fn send(&self, to_id: usize, message: Message<MathAction>) {
        if let Some(transmitter) = self.transmitters.get(&to_id) {
            match transmitter.send(message) {
                Ok(_) => {}
                Err(t) => println!("{}", t),
            }
        }
    }

    fn receive_timeout(&self, timeout: std::time::Duration) -> Option<Message<MathAction>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(t) => Some(t),
            Err(_) => None,
        }
    }

    fn get_actions(&self) -> Vec<MathAction> {
        match self.tasks.try_recv() {
            Ok(t) => vec![t; 1],
            Err(_) => vec![],
        }
    }
}

// Start a simple cluster with 3 replicas. The distributed state machine
// maintains a number that a user can add to or subtract from.
fn main() {
    // Create transmitter and receivers that replicas will be communicating
    // through. In these example, replicas communicate over mspc channels.
    let mut transmitters = BTreeMap::new();
    let mut receivers = BTreeMap::new();
    for i in 0..=2 {
        let (tx, rx) = channel::<Message<MathAction>>();
        transmitters.insert(i, tx);
        receivers.insert(i, rx);
    }

    // Create a cluster abstraction and an mspc channel for each of the
    // replicas. The channel is used to send mathematical operations for the
    // cluster to process to replicas.
    let mut clusters = BTreeMap::new();
    let mut task_transmitters = BTreeMap::new();
    for i in 0..=2 {
        let (task_tx, task_rx) = channel::<MathAction>();
        task_transmitters.insert(i, task_tx);
        clusters.insert(
            i,
            MyCluster {
                receiver: receivers.remove(&i).unwrap(),
                transmitters: transmitters.clone(),
                tasks: task_rx,
            },
        );
    }

    for i in (0..2).rev() {
        let cluster = clusters.remove(&i).unwrap();
        let peer_ids = transmitters.keys().cloned().filter(|id| id != &i).collect();
        thread::spawn(move || {
            Replica::new(
                i,
                peer_ids,
                Box::new(cluster),
                Box::new(Calculator { value: 0 }),
                MathAction { delta: 0 },
            )
            .start(
                ELECTION_MIN_TIMEOUT,
                ELECTION_MAX_TIMEOUT,
                std::time::Duration::from_millis(HEARTBEAT_TIMEOUT),
            );
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
fn process_control_messages(transmitters: BTreeMap<usize, Sender<MathAction>>) {
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
            match command.as_str() {
                "Apply" => {
                    transmitters
                        .get(&id)
                        .unwrap()
                        .send(MathAction { delta: delta })
                        .unwrap_or_else(|error| {
                            println!("{}", error);
                        });
                }
                _ => {}
            };
        }

        thread::sleep(std::time::Duration::from_millis(100));
    }
}
