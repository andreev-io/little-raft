use rand::{thread_rng, Rng};
use std::{sync::mpsc, thread, time::Duration};

enum State {
    Follower,
    Candidate,
    Leader,
    Dead,
}

#[derive(Clone, Debug)]
pub enum Message {
    AppendEntryRequest,
    AppendEntryResponse,
    RequestVoteRequest,
    RequestVoteResponse,
}

#[derive(Clone)]
pub struct Peer {
    id: usize,
    tx: mpsc::Sender<Message>,
}

impl Peer {
    pub fn new(id: usize, tx: mpsc::Sender<Message>) -> Peer {
        Peer { id: id, tx: tx }
    }
}

pub struct ConsensusModule {
    // ID of this replica.
    id: usize,
    // Current term.
    current_term: usize,
    // Votes for self.
    current_votes: usize,
    // Receiving end of a multiple producer single consumer channel.
    rx: mpsc::Receiver<Message>,
    // State of this replica.
    state: State,
    // Vector of peers, i.e. their IDs and the corresponding transmission ends
    // of mpsc channels.
    peers: Vec<Peer>,
    // Who the last vote was cast for.
    voted_for: i32,
    // Logs are simply the terms when the corresponding command was received by
    // the then-leader.
    log: Vec<usize>,
    // Index of highest log entry known to be committed.
    commit_index: i32,
    // Index of highest log entry applied to the state machine.Peer
    last_applied: i32,
    // For each server, index of the next log entry to send to that server. Only
    // present on leaders.
    next_index: Option<Vec<(usize, usize)>>,
    // For each server, index of highest log entry known to be replicated on
    // that server. Only present on leaders.
    match_index: Option<Vec<(usize, usize)>>,
}

impl ConsensusModule {
    // This function starts the replica and blocks forever.
    pub fn start(id: usize, rx: mpsc::Receiver<Message>, peers: Vec<Peer>) {
        let mut replica = ConsensusModule {
            id: id,
            current_term: 0,
            current_votes: 0,
            rx: rx,
            state: State::Follower,
            peers: peers,
            voted_for: -1,
            log: Vec::new(),
            commit_index: -1,
            last_applied: -1,
            next_index: None,
            match_index: None,
        };

        replica.wait_message();
    }

    fn wait_message(&mut self) {
        let mut rng = rand::thread_rng();
        loop {
            match self
                .rx
                .recv_timeout(Duration::from_millis(rng.gen_range(150..=350)))
            {
                Ok(msg) => {
                    match self.state {
                        State::Follower => {

                        },
                        State::Candidate => {

                        },
                        State::Leader => {

                        },
                        State::Dead => {

                        },
                    }
                }
                Err(_) => {
                    // Increase current term. Claim yourself a candidate. Cast a
                    // vote for yourself. Tell every peer to vote for you.
                    self.current_term += 1;
                    self.state = State::Candidate;
                    self.current_votes += 1;
                    for peer in &self.peers {
                        peer.tx
                            .send(Message::RequestVoteRequest)
                            .expect(&format!("Failed to send from {} to {}", self.id, peer.id));
                    }
                }
            }
        }
    }
}
