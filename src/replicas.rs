use crate::types::{ControlMessage, Log, Message, Peer, State, LeaderTimer};
use crossbeam::channel::Receiver;
use crossbeam_channel::Select;
use rand::Rng;
use std::{collections::BTreeSet, time::Duration};

pub struct Replica {
    // This is simply the value the consistency of which the consensus
    // maintains.
    value: i32,
    // ID of this replica.
    id: usize,
    // Current term.
    current_term: usize,
    // ID of peers with votes for self.
    current_votes: Option<Box<BTreeSet<usize>>>,
    // Receiving end of a multiple producer single consumer channel for the Raft
    // protocol.
    rx: Receiver<Message>,
    // Receiving end of a channel for forced state change messages.
    rx_control: Receiver<ControlMessage>,
    // State of this replica.
    state: State,
    // State before dying.
    prev_state: State,
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
    // Timer to times heartbeat messages on the leader.
    leader_timer: LeaderTimer,
}

impl Replica {
    // This function starts the replica and blocks forever.
    pub fn start(
        id: usize,
        rx: Receiver<Message>,
        rx_control: Receiver<ControlMessage>,
        peers: Vec<Peer>,
    ) {
        let mut module = Replica {
            value: 0,
            id: id,
            current_term: 0,
            current_votes: None,
            rx: rx,
            rx_control: rx_control,
            state: State::Follower,
            prev_state: State::Dead,
            peers: peers,
            voted_for: None,
            log: vec![
                Log {
                    index: 0,
                    delta: 0,
                    term: 0
                };
                1
            ],
            commit_index: 0,
            last_applied: 0,
            next_index: None,
            match_index: None,
            leader_timer: LeaderTimer::new(Duration::from_millis(500)),
        };

        module.poll();
    }

    fn send_message(&self, peer_id: usize, message: Message) {
        self.get_peer_by_id(peer_id).send(message);
    }

    fn broadcast_message(&self, message: Message) {
        for peer in &self.peers {
            peer.send(message.clone());
        }
    }

    fn new_append_entry_message(&self) -> Message {
        Message::AppendEntryRequest {
            term: self.current_term,
            from_id: self.id,
            prev_log_index: self.log.len() - 1,
            prev_log_term: self.log[self.log.len() - 1].term,
            entries: Vec::new(),
            commit_index: self.commit_index,
        }
    }

    fn read_message_with_timeout(
        &self,
        timeout: Duration,
    ) -> (Option<Message>, Option<ControlMessage>) {
        let mut select = Select::new();
        let (oper1, oper2) = (select.recv(&self.rx), select.recv(&self.rx_control));
        match select.select_timeout(timeout) {
            Ok(oper) => match oper.index() {
                i if i == oper1 => match oper.recv(&self.rx) {
                    Ok(msg) => (Some(msg), None),
                    Err(_) => panic!("unexpected error"),
                },
                i if i == oper2 => match oper.recv(&self.rx_control) {
                    Ok(msg) => (None, Some(msg)),
                    Err(_) => panic!("unexpected error"),
                },
                _ => unreachable!(),
            },
            _ => (None, None),
        }
    }

    fn poll(&mut self) {
        let mut rng = rand::thread_rng();
        loop {
            match self.state {
                State::Leader => {
                    if self.leader_timer.fired() {
                        self.broadcast_message(self.new_append_entry_message());
                        self.leader_timer.renew();
                    }

                    let timeout = Duration::from_millis(50);
                    let messages = self.read_message_with_timeout(timeout);
                    match messages {
                        (Some(msg), None) => self.process_message_as_leader(msg),
                        (None, Some(msg)) => self.process_control_message(msg),
                        (None, None) => {}
                        (_, _) => unreachable!(),
                    };
                }
                State::Follower => {
                    let timeout = Duration::from_millis(rng.gen_range(1000..=1500));
                    let messages = self.read_message_with_timeout(timeout);
                    match messages {
                        (None, None) => self.become_candidate(),
                        (Some(msg), None) => self.process_message_as_follower(msg),
                        (None, Some(msg)) => self.process_control_message(msg),
                        (_, _) => unreachable!(),
                    };
                }
                State::Candidate => {
                    let timeout = Duration::from_millis(rng.gen_range(1000..=1500));
                    let messages = self.read_message_with_timeout(timeout);
                    match messages {
                        (None, None) => self.become_candidate(),
                        (Some(msg), None) => self.process_message_as_candidate(msg),
                        (None, Some(msg)) => self.process_control_message(msg),
                        (_, _) => unreachable!(),
                    };
                }
                State::Dead => {
                    let message = self.rx_control.recv().unwrap();
                    self.process_control_message(message);
                }
            }
        }
    }

    fn process_control_message(&mut self, message: ControlMessage) {
        match message {
            ControlMessage::Up => self.become_alive(),
            ControlMessage::Down => self.become_dead(),
        }
    }

    fn process_message_as_leader(&mut self, message: Message) {
        match message {
            Message::AppendEntryResponse {
                from_id,
                success,
                term,
            } => {
                if term > self.current_term {
                    self.become_follower(term);
                }
            }
            _ => {}
        }
    }

    fn process_message_as_follower(&mut self, message: Message) {
        match message {
            Message::RequestVoteRequest {
                from_id,
                term,
                last_log_index,
                last_log_term,
            } => {
                if self.current_term > term {
                    self.send_message(
                        from_id,
                        Message::RequestVoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        },
                    );
                } else if self.current_term < term {
                    self.become_follower(term);
                }

                if self.voted_for == None || self.voted_for == Some(from_id) {
                    if self.log.len() == 0
                        || (self.log[self.log.len() - 1].index <= last_log_index
                            && self.log[self.log.len() - 1].term <= last_log_term)
                    {
                        self.send_message(
                            from_id,
                            Message::RequestVoteResponse {
                                from_id: self.id,
                                term: self.current_term,
                                vote_granted: true,
                            },
                        )
                    } else {
                        self.send_message(
                            from_id,
                            Message::RequestVoteResponse {
                                from_id: self.id,
                                term: self.current_term,
                                vote_granted: false,
                            },
                        );
                    }
                } else {
                    self.send_message(
                        from_id,
                        Message::RequestVoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        },
                    );
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
                // Check that the leader's term is at least as large than ours.
                if self.current_term > term {
                    println!("peer {} is follower and received term {} from {} is smaller than self term {}", self.id, term, from_id, self.current_term);
                    self.send_message(
                        from_id,
                        Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                        },
                    )
                // If our log doesn't contain an entry at prev_log_index with
                // the prev_log_term term, reply false.
                } else if prev_log_index >= self.log.len()
                    || self.log[prev_log_index].term != prev_log_term
                {
                    self.send_message(
                        from_id,
                        Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                        },
                    );
                } else {
                    self.send_message(
                        from_id,
                        Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: true,
                        },
                    );
                }

                for entry in &entries {
                    if entry.index < self.log.len() && entry.term != self.log[entry.index].term {
                        self.log.truncate(entry.index + 1);
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
            .unwrap()]
    }

    fn process_message_as_candidate(&mut self, message: Message) {
        match message {
            Message::AppendEntryRequest { term, from_id, .. } => {
                if term >= self.current_term {
                    self.become_follower(term);
                    self.process_message_as_follower(message);
                } else {
                    println!("peer {} is candidate and received term {} from {} is smaller than self term {}", self.id, term, from_id, self.current_term);
                    self.send_message(
                        from_id,
                        Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                        },
                    )
                }
            }
            Message::RequestVoteRequest { term, from_id, .. } => {
                if term > self.current_term {
                    self.become_follower(term);
                    self.process_message_as_follower(message);
                } else {
                    self.send_message(
                        from_id,
                        Message::RequestVoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        },
                    );
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

    fn become_alive(&mut self) {
        println!("peer {} becoming alive", self.id);
        if self.prev_state == State::Dead {
            self.become_follower(0);
        } else {
            self.state = self.prev_state;
        }
        self.prev_state = State::Dead;
    }

    fn become_dead(&mut self) {
        println!("peer {} becoming dead", self.id);
        self.prev_state = self.state;
        self.state = State::Dead;
    }

    fn become_leader(&mut self) {
        println!(
            "peer {} is now leader with term {}",
            self.id, self.current_term
        );
        self.state = State::Leader;
        self.current_votes = None;
        self.voted_for = None;
        self.next_index = Some(vec![self.log.len(); self.peers.len()]);
        self.match_index = Some(vec![0; self.peers.len()]);
        self.broadcast_message(self.new_append_entry_message())
    }

    fn become_follower(&mut self, term: usize) {
        println!("peer {} is now follower with term {}", self.id, term);
        self.current_term = term;
        self.state = State::Follower;
        self.current_votes = None;
        self.voted_for = None;
        self.next_index = None;
        self.match_index = None;
    }

    fn become_candidate(&mut self) {
        println!(
            "peer {} is now candidate with term {}",
            self.id,
            self.current_term + 1
        );
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
        self.broadcast_message(self.new_request_vote_message());
    }

    fn new_request_vote_message(&self) -> Message {
        Message::RequestVoteRequest {
            from_id: self.id,
            term: self.current_term,
            last_log_index: 0, /* TODO: fix */
            last_log_term: 0,  /* TODO: fix */
        }
    }
}
