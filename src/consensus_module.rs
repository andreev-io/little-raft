use rand::{thread_rng, Rng};
use std::{collections::BTreeSet, sync::mpsc, thread, time::Duration};

enum State {
    Follower,
    Candidate,
    Leader,
    Dead,
}

#[derive(Clone, Debug)]
pub enum Message {
    AppendEntryRequest {
        from_id: usize,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<usize>,
        commit_index: usize,
    },
    AppendEntryResponse {
        from_id: usize,
        term: usize,
        success: bool,
    },
    RequestVoteRequest {
        from_id: usize,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    },
    RequestVoteResponse {
        from_id: usize,
        term: usize,
        vote_granted: bool,
    },
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
    // ID of peers with votes for self.
    current_votes: Option<Box<BTreeSet<usize>>>,
    // Receiving end of a multiple producer single consumer channel.
    rx: mpsc::Receiver<Message>,
    // State of this replica.
    state: State,
    // Vector of peers, i.e. their IDs and the corresponding transmission ends
    // of mpsc channels.
    peers: Vec<Peer>,
    // Who the last vote was cast for.
    voted_for: Option<usize>,
    // Logs are simply the terms when the corresponding command was received by
    // the then-leader.
    log: Vec<usize>,
    // Index of highest log entry known to be committed.
    commit_index: usize,
    // Index of highest log entry applied to the state machine.Peer
    last_applied: usize,
    // For each server, index of the next log entry to send to that server. Only
    // present on leaders.
    next_index: Option<Vec<(usize, usize)>>,
    // For each server, index of highest log entry known to be replicated on
    // that server. Only present on leaders.
    match_index: Option<Vec<(usize, usize)>>,
}

impl ConsensusModule {
    // This function starts the replica and blocks forever.
    pub fn start(id: usize, rx: mpsc::Receiver<Message>, mut peers: Vec<Peer>) {
        peers.sort_by_key(|k| k.id);

        let mut replica = ConsensusModule {
            id: id,
            current_term: 0,
            current_votes: None,
            rx: rx,
            state: State::Follower,
            peers: peers,
            voted_for: None,
            log: Vec::new(),
            commit_index: 0,
            last_applied: 0,
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
                Ok(msg) => match &self.state {
                    State::Candidate => self.process_message_as_candidate(msg),
                    State::Follower => self.process_message_as_follower(msg),
                    State::Leader => {}
                    State::Dead => {}
                },
                Err(_) => self.become_candidate(),
            }
        }
    }

    fn process_message_as_leader(&mut self, message: Message) {
        match message {
            _ => {}
        }
    }

    fn process_message_as_follower(&mut self, message: Message) {
        match message {
            Message::AppendEntryRequest { .. } => {}
            Message::RequestVoteRequest { .. } => {}
            Message::AppendEntryResponse { .. } => { /* ignore */ }
            Message::RequestVoteResponse { .. } => { /* ignore */ }
        }
    }

    fn get_peer_by_id(&self, peer_id: usize) -> &Peer {
        &self.peers[self
            .peers
            .binary_search_by_key(&peer_id, |peer| peer.id)
            .expect("Could not find the peer")]
    }

    fn process_message_as_candidate(&mut self, message: Message) {
        match message {
            Message::AppendEntryRequest { term, from_id, .. } => {
                if term >= self.current_term {
                    self.become_follower(term);
                    self.process_message_as_follower(message);
                } else {
                    let peer = self.get_peer_by_id(from_id);
                    peer.tx
                        .send(Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                        })
                        .expect("Could not send a message");
                }
            }
            Message::RequestVoteRequest { term, from_id, .. } => {
                if term > self.current_term {
                    self.become_follower(term);
                    self.process_message_as_follower(message);
                } else {
                    let peer = self.get_peer_by_id(from_id);
                    peer.tx
                        .send(Message::RequestVoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        })
                        .expect("Could not send a message");
                }
            }
            Message::RequestVoteResponse {
                from_id,
                term,
                vote_granted,
            } => {
                if term > self.current_term {
                    self.become_follower(term);
                } else if vote_granted {
                    if let Some(cur_votes) = &mut self.current_votes {
                        cur_votes.insert(from_id);
                        if cur_votes.len() * 2 > self.peers.len() + 1 {
                            // become leader
                        }
                    }
                }
            }
            Message::AppendEntryResponse { .. } => { /* ignore */ }
        }
    }

    fn become_follower(&mut self, term: usize) {
        self.current_term = term;
        self.state = State::Follower;
        self.current_votes = None;
        self.voted_for = None;
    }

    fn become_candidate(&mut self) {
        // Increase current term.
        self.current_term += 1;
        // Claim yourself a candidate.
        self.state = State::Candidate;
        // Initialize votes. Vote for yourself.
        let mut votes = BTreeSet::new();
        votes.insert(self.id);
        self.current_votes = Some(Box::new(votes));
        self.voted_for = Some(self.id);
        // Fan out vote requests.
        for peer in &self.peers {
            peer.tx
                .send(Message::RequestVoteRequest {
                    from_id: self.id,
                    term: self.current_term,
                    last_log_index: 0, /* TODO: fix */
                    last_log_term: 0,  /* TODO: fix */
                })
                .expect(&format!("Failed to send from {} to {}", self.id, peer.id));
        }
    }
}
