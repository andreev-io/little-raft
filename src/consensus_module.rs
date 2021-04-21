use crossbeam::channel;
use crossbeam_channel::{unbounded, RecvTimeoutError, Select};
use rand::{thread_rng, Rng};
use std::{collections::BTreeSet, thread, time::Duration};

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
        entries: Vec<Log>,
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
    tx: channel::Sender<Message>,
}

impl Peer {
    pub fn new(id: usize, tx: channel::Sender<Message>) -> Peer {
        Peer { id: id, tx: tx }
    }
}

#[derive(Debug, Clone)]
struct Log {
    index: usize,
    delta: i32,
    term: usize,
}

pub struct ConsensusModule {
    // This is simply the value the consistency of which the consensus
    // maintains.
    value: i32,
    // ID of this replica.
    id: usize,
    // Current term.
    current_term: usize,
    // ID of peers with votes for self.
    current_votes: Option<Box<BTreeSet<usize>>>,
    // Receiving end of a multiple producer single consumer channel.
    rx: channel::Receiver<Message>,
    // State of this replica.
    state: State,
    // Vector of peers, i.e. their IDs and the corresponding transmission ends
    // of mpsc channels.
    peers: Vec<Peer>,
    // Who the last vote was cast for.
    voted_for: Option<usize>,
    // Logs are simply the terms when the corresponding command was received by
    // the then-leader.
    log: Vec<Log>,
    // Index of highest log entry known to be committed.
    commit_index: usize,
    // Index of highest log entry applied to the state machine.
    last_applied: usize,
    // For each server, index of the next log entry to send to that server. Only
    // present on leaders.
    next_index: Option<Vec<usize>>,
    // For each server, index of highest log entry known to be replicated on
    // that server. Only present on leaders.
    match_index: Option<Vec<usize>>,
}

impl ConsensusModule {
    // This function starts the replica and blocks forever.
    pub fn start(id: usize, rx: channel::Receiver<Message>, mut peers: Vec<Peer>) {
        peers.sort_by_key(|k| k.id);

        let mut module = ConsensusModule {
            value: 0,
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

        module.wait_message();
    }

    fn wait_message(&mut self) {
        let mut rng = rand::thread_rng();
        loop {
            match self.state {
                State::Leader => {
                    match self.rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(msg) => self.process_message_as_leader(msg),
                        Err(RecvTimeoutError::Timeout) => {
                            for peer in &self.peers {
                                peer.tx
                                    .send(Message::AppendEntryRequest {
                                        term: self.current_term,
                                        from_id: self.id,
                                        prev_log_index: if self.log.len() > 0 {
                                            self.log.len() - 1
                                        } else {
                                            0
                                        },
                                        prev_log_term: if self.log.len() > 0 {
                                            self.log[self.log.len() - 1].term
                                        } else {
                                            0
                                        },
                                        entries: Vec::new(),
                                        commit_index: self.commit_index,
                                    })
                                    .expect("Failed to send a message");
                            }
                        }
                        Err(_) => panic!("unexpected error"),
                    };
                }
                State::Dead => {}
                State::Candidate | State::Follower => {
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
        }
    }

    fn process_message_as_leader(&mut self, message: Message) {
        match message {
            _ => {}
        }
    }

    fn process_message_as_follower(&mut self, mut message: Message) {
        match message {
            Message::RequestVoteRequest {
                from_id,
                term,
                last_log_index,
                last_log_term,
            } => {
                if self.current_term > term {
                    let peer = self.get_peer_by_id(from_id);
                    peer.tx
                        .send(Message::RequestVoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        })
                        .expect("Could not send a message");
                } else if self.current_term < term {
                    self.become_follower(term);
                }

                let peer = self.get_peer_by_id(from_id);
                if self.voted_for == None || self.voted_for == Some(from_id) {
                    if self.log.len() == 0
                        || (self.log[self.log.len() - 1].index <= last_log_index
                            && self.log[self.log.len() - 1].term <= last_log_term)
                    {
                        peer.tx
                            .send(Message::RequestVoteResponse {
                                from_id: self.id,
                                term: self.current_term,
                                vote_granted: true,
                            })
                            .expect("Could not send a message");
                    } else {
                        peer.tx
                            .send(Message::RequestVoteResponse {
                                from_id: self.id,
                                term: self.current_term,
                                vote_granted: false,
                            })
                            .expect("Could not send a message");
                    }
                } else {
                    peer.tx
                        .send(Message::RequestVoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        })
                        .expect("Could not send a message");
                }
            }
            Message::AppendEntryRequest {
                term,
                from_id,
                prev_log_index,
                prev_log_term,
                mut entries,
                commit_index,
            } => {
                let peer = self.get_peer_by_id(from_id);
                // Check that the leader's term is at least as large than ours.
                if self.current_term > term {
                    peer.tx
                        .send(Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                        })
                        .expect("Could not send a message");
                // If our log doesn't contain an entry at prev_log_index with
                // the prev_log_term term, reply false.
                } else if prev_log_index >= self.log.len()
                    || self.log[prev_log_index].term != prev_log_term
                {
                    peer.tx
                        .send(Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                        })
                        .expect("Could not send a message");
                } else {
                    peer.tx
                        .send(Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: true,
                        })
                        .expect("Could not send a message");
                }

                for entry in &entries {
                    if entry.index < self.log.len() && entry.term != self.log[entry.index].term {
                        self.log.split_off(entry.index);
                    }
                }

                self.log.append(&mut entries);
                if commit_index > self.commit_index && self.log.len() != 0 {
                    self.commit_index = if commit_index < self.log[self.log.len() - 1].index {
                        commit_index
                    } else {
                        self.log[self.log.len() - 1].index
                    }
                }

                while self.last_applied < commit_index && self.last_applied < self.log.len() - 1 {
                    self.value += self.log[self.last_applied + 1].delta;
                    self.last_applied += 1;
                }
            }
            Message::AppendEntryResponse { .. } => { /* ignore */ }
            Message::RequestVoteResponse { .. } => { /* ignore */ }
        }
    }

    fn get_peer_by_id(&self, peer_id: usize) -> &Peer {
        &self.peers[self
            .peers
            .binary_search_by_key(&peer_id, |peer| peer.id)
            .expect("Could not find peer")]
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
                            self.become_leader();
                        }
                    }
                }
            }
            Message::AppendEntryResponse { .. } => { /* ignore */ }
        }
    }

    fn become_leader(&mut self) {
        self.state = State::Leader;
        self.current_votes = None;
        self.voted_for = None;
        self.next_index = Some(vec![self.log.len(); self.peers.len()]);
        self.match_index = Some(vec![0; self.peers.len()]);
    }

    fn become_follower(&mut self, term: usize) {
        self.current_term = term;
        self.state = State::Follower;
        self.current_votes = None;
        self.voted_for = None;
        self.next_index = None;
        self.match_index = None;
    }

    fn become_candidate(&mut self) {
        self.next_index = None;
        self.match_index = None;
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
