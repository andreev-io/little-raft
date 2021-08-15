use crate::{
    cluster::Cluster, heartbeat_timer::HeartbeatTimer, message::Message,
    state_machine::StateMachine,
};
use rand::Rng;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Follower,
    Candidate,
    Leader,
}

#[derive(Clone, Debug)]
pub struct Action<A> {
    action: A,
    index: usize,
    term: usize,
}

#[derive(Debug)]
pub struct Replica<S, A>
where
    S: StateMachine<A>,
{
    peer_ids: Vec<usize>,
    state_machine: Box<S>,
    cluster: Box<dyn Cluster<A>>,
    // ID of this replica.
    id: usize,
    // Current term.
    current_term: usize,
    // ID of peers with votes for self.
    current_votes: Option<Box<BTreeSet<usize>>>,
    // State of this replica.
    state: State,
    // Who the last vote was cast for.
    voted_for: Option<usize>,
    // Action along with its index and term.
    actions: Vec<Action<A>>,
    // Index of the highest log entry known to be committed.
    commit_index: usize,
    // Index of the highest log entry applied to the state machine.
    last_applied: usize,
    // For each server, index of the next log entry to send to that server. Only
    // present on leaders.
    next_index: BTreeMap<usize, usize>,
    // For each server, index of highest log entry known to be replicated on
    // that server. Only present on leaders.
    match_index: BTreeMap<usize, usize>,
    // Timer to times heartbeat messages on the leader.
    heartbeat_timer: HeartbeatTimer,
}

impl<S, A> Replica<S, A>
where
    S: StateMachine<A> + std::fmt::Debug,
    A: Copy + std::fmt::Debug,
{
    pub fn new(
        id: usize,
        peer_ids: Vec<usize>,
        cluster: Box<dyn Cluster<A>>,
        state_machine: Box<S>,
        null_action: A,
        heartbeat_timeout: std::time::Duration,
    ) -> Replica<S, A> {
        Replica {
            state_machine: state_machine,
            cluster: cluster,
            peer_ids: peer_ids,
            id: id,
            current_term: 0,
            current_votes: None,
            state: State::Follower,
            voted_for: None,
            actions: vec![Action {
                term: 0,
                index: 0,
                action: null_action,
            }],
            commit_index: 0,
            last_applied: 0,
            next_index: BTreeMap::new(),
            match_index: BTreeMap::new(),
            heartbeat_timer: HeartbeatTimer::new(heartbeat_timeout),
        }
    }

    // This function starts the replica and blocks forever.
    pub fn start(&mut self, min_election_timeout: u64, max_election_timeout: u64) {
        self.poll(min_election_timeout, max_election_timeout);
    }

    fn broadcast_message<F>(&self, message_generator: F)
    where
        F: Fn(usize) -> Message<A>,
    {
        for peer_id in &self.peer_ids {
            self.cluster
                .send(peer_id.clone(), message_generator(peer_id.clone()))
        }
    }

    fn read_message_with_timeout(&self, timeout: Duration) -> Option<Message<A>> {
        let msg = self.cluster.receive_timeout(timeout);
        println!("Node {} got MESSAGE: {:?}", self.id, msg);
        return msg;
    }

    fn get_entries_for_peer(&self, peer_id: usize) -> Vec<Action<A>> {
        self.actions[self.next_index[&peer_id]..self.actions.len()].to_vec()
    }

    fn poll(&mut self, min_election_timeout: u64, max_election_timeout: u64) {
        let mut rng = rand::thread_rng();
        loop {
            println!("{:?}", self);
            match self.state {
                State::Leader => {
                    if self.heartbeat_timer.fired() {
                        self.broadcast_message(|id: usize| Message::AppendEntryRequest {
                            term: self.current_term,
                            from_id: self.id,
                            prev_log_index: self.next_index[&id] - 1,
                            prev_log_term: self.actions[self.next_index[&id] - 1].term,
                            entries: self.get_entries_for_peer(id),
                            commit_index: self.commit_index,
                        });
                        self.heartbeat_timer.renew();
                    }

                    let timeout = self.heartbeat_timer.timeout;
                    let message = self.read_message_with_timeout(timeout);
                    if let Some(msg) = message {
                        self.process_message_as_leader(msg)
                    };
                }
                State::Follower => {
                    let timeout = Duration::from_millis(
                        rng.gen_range(min_election_timeout..=max_election_timeout),
                    );
                    let message = self.read_message_with_timeout(timeout);
                    if let Some(msg) = message {
                        self.process_message_as_follower(msg);
                    } else {
                        self.become_candidate();
                    }
                }
                State::Candidate => {
                    let timeout = Duration::from_millis(
                        rng.gen_range(min_election_timeout..=max_election_timeout),
                    );
                    let message = self.read_message_with_timeout(timeout);
                    if let Some(msg) = message {
                        self.process_message_as_candidate(msg);
                    } else {
                        self.become_candidate();
                    }
                }
            }

            self.apply_ready_changes();
            self.process_action();
            std::thread::sleep(std::time::Duration::from_millis(5000));
        }
    }

    fn apply_ready_changes(&mut self) {
        // Move the commit index to the latest log index that has been
        // replicated on the majority of the replicas.
        if self.state == State::Leader && self.commit_index < self.actions.len() - 1 {
            let mut n = self.actions.len() - 1;
            while n > self.commit_index {
                let num_replications =
                    self.match_index.iter().fold(
                        0,
                        |acc, mtch_idx| if mtch_idx.1 >= &n { acc + 1 } else { acc },
                    );

                if num_replications * 2 >= self.peer_ids.len()
                    && self.actions[n].term == self.current_term
                {
                    self.commit_index = n;
                }
                n -= 1;
            }
        }

        // Apply changes that are behind the currently committed index.
        while self.commit_index > self.last_applied {
            self.last_applied += 1;
            self.state_machine
                .apply_action(self.actions[self.last_applied].action);
        }
    }

    fn process_action(&mut self) {
        if let Some(a) = self.cluster.get_action() {
            if self.state == State::Leader {
                self.actions.push(Action {
                    index: self.actions.len(),
                    action: a,
                    term: self.current_term,
                });
            }
        }
    }

    fn process_message_as_leader(&mut self, message: Message<A>) {
        match message {
            Message::AppendEntryResponse {
                from_id,
                success,
                term,
                last_index,
            } => {
                if term > self.current_term {
                    self.become_follower(term);
                } else if success {
                    self.next_index.insert(from_id, last_index + 1);
                    self.match_index.insert(from_id, last_index);
                } else {
                    self.next_index
                        .insert(from_id, self.next_index[&from_id] - 1);
                }
            }
            _ => {}
        }
    }

    fn process_request_vote_request_as_follower(
        &mut self,
        from_id: usize,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    ) {
        if self.current_term > term {
            self.cluster.send(
                from_id,
                Message::VoteResponse {
                    from_id: self.id,
                    term: self.current_term,
                    vote_granted: false,
                },
            );
        } else if self.current_term < term {
            self.become_follower(term);
        }

        if self.voted_for == None || self.voted_for == Some(from_id) {
            if self.actions[self.actions.len() - 1].index <= last_log_index
                && self.actions[self.actions.len() - 1].term <= last_log_term
            {
                self.cluster.send(
                    from_id,
                    Message::VoteResponse {
                        from_id: self.id,
                        term: self.current_term,
                        vote_granted: true,
                    },
                );
            } else {
                self.cluster.send(
                    from_id,
                    Message::VoteResponse {
                        from_id: self.id,
                        term: self.current_term,
                        vote_granted: false,
                    },
                );
            }
        } else {
            self.cluster.send(
                from_id,
                Message::VoteResponse {
                    from_id: self.id,
                    term: self.current_term,
                    vote_granted: false,
                },
            )
        }
    }

    fn process_append_entry_request_as_follower(
        &mut self,
        from_id: usize,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<Action<A>>,
        commit_index: usize,
    ) {
        // Check that the leader's term is at least as large as ours.
        if self.current_term > term {
            self.cluster.send(
                from_id,
                Message::AppendEntryResponse {
                    from_id: self.id,
                    term: self.current_term,
                    success: false,
                    last_index: self.actions.len() - 1,
                },
            );

            return;
        // If our log doesn't contain an entry at prev_log_index with
        // the prev_log_term term, reply false.
        } else if prev_log_index >= self.actions.len()
            || self.actions[prev_log_index].term != prev_log_term
        {
            self.cluster.send(
                from_id,
                Message::AppendEntryResponse {
                    from_id: self.id,
                    term: self.current_term,
                    success: false,
                    last_index: self.actions.len() - 1,
                },
            );

            return;
        }

        for entry in entries {
            if entry.index < self.actions.len() && entry.term != self.actions[entry.index].term {
                self.actions.truncate(entry.index);
            }

            if entry.index == self.actions.len() {
                self.actions.push(entry);
            }
        }

        if commit_index > self.commit_index && self.actions.len() != 0 {
            self.commit_index = if commit_index < self.actions[self.actions.len() - 1].index {
                commit_index
            } else {
                self.actions[self.actions.len() - 1].index
            }
        }

        self.cluster.send(
            from_id,
            Message::AppendEntryResponse {
                from_id: self.id,
                term: self.current_term,
                success: true,
                last_index: self.actions.len() - 1,
            },
        );
    }

    fn process_message_as_follower(&mut self, message: Message<A>) {
        match message {
            Message::VoteRequest {
                from_id,
                term,
                last_log_index,
                last_log_term,
            } => self.process_request_vote_request_as_follower(
                from_id,
                term,
                last_log_index,
                last_log_term,
            ),
            Message::AppendEntryRequest {
                term,
                from_id,
                prev_log_index,
                prev_log_term,
                entries,
                commit_index,
            } => self.process_append_entry_request_as_follower(
                from_id,
                term,
                prev_log_index,
                prev_log_term,
                entries,
                commit_index,
            ),
            Message::AppendEntryResponse { .. } => { /* ignore */ }
            Message::VoteResponse { .. } => { /* ignore */ }
        }
    }

    fn process_message_as_candidate(&mut self, message: Message<A>) {
        match message {
            Message::AppendEntryRequest { term, from_id, .. } => {
                if term >= self.current_term {
                    self.become_follower(term);
                    self.process_message_as_follower(message);
                } else {
                    self.cluster.send(
                        from_id,
                        Message::AppendEntryResponse {
                            from_id: self.id,
                            term: self.current_term,
                            success: false,
                            last_index: self.actions.len() - 1,
                        },
                    );
                }
            }
            Message::VoteRequest { term, from_id, .. } => {
                if term > self.current_term {
                    self.become_follower(term);
                    self.process_message_as_follower(message);
                } else {
                    self.cluster.send(
                        from_id,
                        Message::VoteResponse {
                            from_id: self.id,
                            term: self.current_term,
                            vote_granted: false,
                        },
                    );
                }
            }
            Message::VoteResponse {
                from_id,
                term,
                vote_granted,
            } => {
                if term > self.current_term {
                    self.become_follower(term);
                } else if vote_granted {
                    if let Some(cur_votes) = &mut self.current_votes {
                        cur_votes.insert(from_id);
                        if cur_votes.len() * 2 >= self.peer_ids.len() {
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
        self.next_index = BTreeMap::new();
        self.match_index = BTreeMap::new();
        for peer_id in &self.peer_ids {
            self.next_index.insert(peer_id.clone(), self.actions.len());
            self.match_index.insert(peer_id.clone(), 0);
        }

        // If the previous leader had some uncommitted entries that were
        // replicated to this now-leader server, this server will not commit
        // them until its commit index advanced to a log entry appended in this
        // leader's term. To carry out this operation as soon as the new leader
        // emerges, append a no-op entry (part 8 of the paper).
        // self.append(0);
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
        println!("BROADCASTING MESSAGES");
        self.broadcast_message(|_: usize| Message::VoteRequest {
            from_id: self.id,
            term: self.current_term,
            last_log_index: self.actions.len() - 1,
            last_log_term: self.actions[self.actions.len() - 1].term,
        });
    }
}
