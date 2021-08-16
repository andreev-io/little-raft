use crossbeam_channel::{unbounded, Receiver, SendError, Sender};
use little_raft::{
    cluster::Cluster,
    message::Message,
    replica::Replica,
    state_machine::{StateMachine, StateMachineTransition, TransitionState},
};

use std::{collections::BTreeMap, thread};

const HEARTBEAT_TIMEOUT: u64 = 200;
const ELECTION_MIN_TIMEOUT: u64 = 500;
const ELECTION_MAX_TIMEOUT: u64 = 700;

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
    value_tx: Sender<i32>,
    applied_ids_tx: Sender<(usize, usize)>,
}

impl StateMachine<ArithmeticOperation> for Calculator {
    fn apply_transition(&mut self, transition: ArithmeticOperation) {
        self.value += transition.delta;
        self.value_tx
            .send(self.value)
            .expect("could not send calculator value");
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
    receiver: Receiver<Message<ArithmeticOperation>>,
    transmitters: BTreeMap<usize, Sender<Message<ArithmeticOperation>>>,
    tasks: Receiver<ArithmeticOperation>,
    id: usize,
    leader: Option<usize>,
}

impl Cluster<ArithmeticOperation> for MyCluster {
    fn send(&self, to_id: usize, message: Message<ArithmeticOperation>) {
        if let Some(transmitter) = self.transmitters.get(&to_id) {
            match transmitter.send(message) {
                Ok(_) => {}
                Err(t) => println!("{}", t),
            }
        }
    }

    fn receive_timeout(
        &self,
        timeout: std::time::Duration,
    ) -> Option<Message<ArithmeticOperation>> {
        match self.receiver.recv_timeout(timeout) {
            Ok(t) => Some(t),
            Err(_) => None,
        }
    }

    fn get_pending_transitions(&self) -> Vec<ArithmeticOperation> {
        if let Some(cur_leader) = self.leader {
            if cur_leader == self.id {
                return match self.tasks.try_recv() {
                    Ok(t) => vec![t; 1],
                    Err(_) => vec![],
                };
            }
        }

        Vec::new()
    }

    fn register_leader_change(&mut self, leader_id: Option<usize>) {
        self.leader = leader_id;
    }
}

#[test]
fn run_replicas() -> Result<(), SendError<ArithmeticOperation>> {
    // Create transmitters and receivers that replicas will be communicating
    // through. In this test, replicas communicate over mspc channels.
    let (mut transmitters, mut receivers) = (BTreeMap::new(), BTreeMap::new());
    for i in 0..=2 {
        let (tx, rx) = unbounded::<Message<ArithmeticOperation>>();
        transmitters.insert(i, tx);
        receivers.insert(i, rx);
    }

    // Create clusters and mspc channel for each of the replicas. The channels
    // are used to send mathematical operations for the cluster to pipe to the
    // replicas.
    let (task_tx, task_rx) = unbounded::<ArithmeticOperation>();
    let (calculator_tx, _calculator_rx) = unbounded::<i32>();
    let (applied_tx, applied_rx) = unbounded::<(usize, usize)>();
    for i in 0..=2 {
        let cluster = MyCluster {
            id: i,
            receiver: receivers.remove(&i).unwrap(),
            transmitters: transmitters.clone(),
            tasks: task_rx.clone(),
            leader: None,
        };

        let mut peer_ids = Vec::new();
        for n in 0..=2 {
            if n != i {
                peer_ids.push(n);
            }
        }

        let new_calculator_tx = calculator_tx.clone();
        let new_applied_tx = applied_tx.clone();
        thread::spawn(move || {
            Replica::new(
                i,
                peer_ids,
                Box::new(cluster),
                Box::new(Calculator {
                    id: i,
                    value: 0,
                    value_tx: new_calculator_tx,
                    applied_ids_tx: new_applied_tx,
                }),
                ArithmeticOperation { delta: 0, id: 0 },
            )
            .start(
                ELECTION_MIN_TIMEOUT,
                ELECTION_MAX_TIMEOUT,
                std::time::Duration::from_millis(HEARTBEAT_TIMEOUT),
            );
        });
    }

    thread::sleep(std::time::Duration::from_secs(2));
    task_tx.send(ArithmeticOperation { delta: 5, id: 1 })?;
    task_tx.send(ArithmeticOperation { delta: -51, id: 2 })?;
    task_tx.send(ArithmeticOperation { delta: -511, id: 3 })?;
    task_tx.send(ArithmeticOperation { delta: 3, id: 4 })?;
    thread::sleep(std::time::Duration::from_secs(2));

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

    Ok(())
}
