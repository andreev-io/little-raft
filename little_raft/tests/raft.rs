use crossbeam_channel as channel;
use crossbeam_channel::{unbounded, Sender};
use little_raft::{
    cluster::Cluster,
    message::Message,
    replica::Replica,
    state_machine::{StateMachine, StateMachineTransition, TransitionState},
};
use std::sync::{Arc, Mutex};

use std::{collections::BTreeMap, thread, time::Duration};

const HEARTBEAT_TIMEOUT: Duration = Duration::from_millis(500);
const ELECTION_MIN_TIMEOUT: Duration = Duration::from_millis(750);
const ELECTION_MAX_TIMEOUT: Duration = Duration::from_millis(950);

#[derive(Clone, Copy, Debug)]
struct ArithmeticOperation {
    delta: i32,
    id: usize,
}

impl StateMachineTransition for ArithmeticOperation {
    type TransitionID = usize;
    fn get_id(&self) -> Self::TransitionID {
        self.id
    }
}

struct Calculator {
    id: usize,
    value: i32,
    applied_ids_tx: Sender<(usize, usize)>,
}

impl StateMachine<ArithmeticOperation> for Calculator {
    fn apply_transition(&mut self, transition: ArithmeticOperation) {
        self.value += transition.delta;
    }

    fn register_transition_state(
        &mut self,
        transition_id: <ArithmeticOperation as StateMachineTransition>::TransitionID,
        state: TransitionState,
    ) {
        if state == TransitionState::Applied {
            self.applied_ids_tx
                .send((self.id, transition_id))
                .expect("could not send applied transition id");
        }
    }
}

struct MyCluster {
    transmitters: BTreeMap<usize, Sender<Message<ArithmeticOperation>>>,
    pending_transitions: Vec<ArithmeticOperation>,
    pending_messages: Vec<Message<ArithmeticOperation>>,
    halt: bool,
    leader: bool,
    id: usize,
}

impl Cluster<ArithmeticOperation> for MyCluster {
    fn register_leader(&mut self, leader_id: Option<usize>) {
        if let Some(id) = leader_id {
            if id == self.id {
                self.leader = true;
            } else {
                self.leader = false;
            }
        } else {
            self.leader = false;
        }
    }

    fn send(&mut self, to_id: usize, message: Message<ArithmeticOperation>) {
        if let Some(transmitter) = self.transmitters.get(&to_id) {
            match transmitter.send(message) {
                Ok(_) => {}
                Err(t) => println!("{}", t),
            }
        }
    }

    fn halt(&self) -> bool {
        self.halt
    }

    fn receive(&mut self) -> Vec<Message<ArithmeticOperation>> {
        let cur = self.pending_messages.clone();
        self.pending_messages = Vec::new();
        cur
    }

    fn get_pending_transitions(&mut self) -> Vec<ArithmeticOperation> {
        let cur = self.pending_transitions.clone();
        self.pending_transitions = Vec::new();
        cur
    }
}

#[test]
fn run_replicas() {
    let (mut transmitters, mut receivers) = (BTreeMap::new(), BTreeMap::new());
    for i in 0..=2 {
        let (tx, rx) = unbounded::<Message<ArithmeticOperation>>();
        transmitters.insert(i, tx);
        receivers.insert(i, rx);
    }

    let (mut state_machines, mut clusters, mut notifiers) = (Vec::new(), Vec::new(), Vec::new());
    let (applied_tx, applied_rx) = unbounded();
    for i in 0..=2 {
        // Create the cluster.
        let cluster = Arc::new(Mutex::new(MyCluster {
            transmitters: transmitters.clone(),
            pending_transitions: Vec::new(),
            pending_messages: Vec::new(),
            halt: false,
            leader: false,
            id: i,
        }));
        clusters.push((cluster.clone(), receivers.remove(&i).unwrap()));

        // Create peer ids.
        let mut peer_ids = Vec::new();
        for n in 0..=2 {
            if n != i {
                peer_ids.push(n);
            }
        }

        // Create the state machine.
        let new_applied_tx = applied_tx.clone();
        let state_machine = Arc::new(Mutex::new(Calculator {
            id: i,
            value: 0,
            applied_ids_tx: new_applied_tx,
        }));

        // Create noop transition.
        let noop = ArithmeticOperation { delta: 0, id: 0 };
        state_machines.push(state_machine.clone());
        let (message_notifier_tx, message_notifier_rx) = channel::unbounded();
        let (transition_notifier_tx, transition_notifier_rx) = channel::unbounded();
        notifiers.push((message_notifier_tx, transition_notifier_tx));
        thread::spawn(move || {
            let mut replica = Replica::new(
                i,
                peer_ids,
                cluster,
                state_machine,
                noop,
                HEARTBEAT_TIMEOUT,
                (ELECTION_MIN_TIMEOUT, ELECTION_MAX_TIMEOUT),
            );
            replica.start(message_notifier_rx, transition_notifier_rx);
        });
    }

    let new_clusters = clusters.clone();
    for (i, (cluster, receiver)) in new_clusters.into_iter().enumerate() {
        let message_notifier = notifiers[i].0.clone();
        thread::spawn(move || loop {
            let msg = receiver.recv().unwrap();
            cluster.lock().unwrap().pending_messages.push(msg);
            let _ = message_notifier.send(());
        });
    }

    thread::sleep(Duration::from_secs(2));
    for (i, (cluster, _)) in clusters.iter().enumerate() {
        let mut c = cluster.lock().unwrap();
        if c.leader {
            c.pending_transitions
                .push(ArithmeticOperation { delta: 5, id: 1 });
            let _ = notifiers[i].1.send(());
            break;
        }
    }

    thread::sleep(Duration::from_secs(2));
    for (i, (cluster, _)) in clusters.iter().enumerate() {
        let mut c = cluster.lock().unwrap();
        if c.leader {
            c.pending_transitions
                .push(ArithmeticOperation { delta: -51, id: 2 });
            let _ = notifiers[i].1.send(());
            break;
        }
    }

    thread::sleep(Duration::from_secs(2));
    for (i, (cluster, _)) in clusters.iter().enumerate() {
        let mut c = cluster.lock().unwrap();
        if c.leader {
            c.pending_transitions
                .push(ArithmeticOperation { delta: -511, id: 3 });
            let _ = notifiers[i].1.send(());
            break;
        }
    }

    thread::sleep(Duration::from_secs(2));
    for (i, (cluster, _)) in clusters.iter().enumerate() {
        let mut c = cluster.lock().unwrap();
        if c.leader {
            c.pending_transitions
                .push(ArithmeticOperation { delta: 3, id: 4 });
            let _ = notifiers[i].1.send(());
            break;
        }
    }

    thread::sleep(Duration::from_secs(2));
    for (cluster, _) in clusters.iter() {
        let mut c = cluster.lock().unwrap();
        c.halt = true;
    }
    thread::sleep(Duration::from_secs(5));

    let applied_transactions: Vec<(usize, usize)> = applied_rx.try_iter().collect();
    let expected_vec: Vec<usize> = vec![0, 1, 2, 3, 4];
    assert_eq!(
        expected_vec,
        applied_transactions.iter().fold(Vec::new(), |mut acc, x| {
            if x.0 == 0 {
                acc.push(x.1);
            };
            acc
        })
    );

    assert_eq!(
        expected_vec,
        applied_transactions.iter().fold(Vec::new(), |mut acc, x| {
            if x.0 == 1 {
                acc.push(x.1);
            };
            acc
        })
    );

    assert_eq!(
        expected_vec,
        applied_transactions.iter().fold(Vec::new(), |mut acc, x| {
            if x.0 == 2 {
                acc.push(x.1);
            };
            acc
        })
    );
}
