use crate::{
    cluster::Cluster,
    heartbeat_timer::HeartbeatTimer,
    message::{Entry, EntryState, Message},
    state_machine::{StateMachine, StateMachineTransition},
};
use rand::Rng;
use std::{
    collections::{BTreeMap, BTreeSet},
    time::Duration,
};

#[derive(Clone, Copy, PartialEq)]
enum State {
    Follower,
    Candidate,
    Leader,
}

pub type ReplicaID = usize;

pub struct Replica<S, T>
where
    T: StateMachineTransition,
    S: StateMachine<T>,
{
    // ID of this replica.
    id: ReplicaID,
    // IDs of other replicas in the cluster.
    peer_ids: Vec<ReplicaID>,
    // User-defined state machine that the cluster replicates.
    state_machine: Box<S>,
    // Interface a replica uses to communicate with the rest of the cluster and
    // the user.
    cluster: Box<dyn Cluster<T>>,
    // Current term.
    current_term: usize,
    // ID of peers with votes for self.
    current_votes: Option<Box<BTreeSet<usize>>>,
    // State of this replica.
    state: State,
    // Who the last vote was cast for.
    voted_for: Option<usize>,
    // entries this replica is aware of.
    entries: Vec<Entry<T>>,
    // Index of the highest transition known to be committed.
    commit_index: usize,
    // Index of the highest transition applied to the state machine.
    last_applied: usize,
    // For each server, index of the next log entry to send to that server. Only
    // present on leaders.
    next_index: BTreeMap<usize, usize>,
    // For each server, index of highest log entry known to be replicated on
    // that server. Only present on leaders.
    match_index: BTreeMap<usize, usize>,
    // No-op transition used to force a faster replica update when a cluster Leader
    // changes.
    noop_transition: T,
    rng: rand::prelude::ThreadRng,
    leader_id: Option<ReplicaID>,
}

// Replica describes a single state machine running the Raft algorithm.
// Instances of replicas communicate with each other to achieve consensus on the
// state of the user-defined state machine.
impl<S, T> Replica<S, T>
where
    T: StateMachineTransition,
    S: StateMachine<T>,
{
    // Create a new Replica. Provide its unique identifier within the cluster, a
    // vector of its peers identifiers (all peers in the cluster), the state
    // machine that the cluster maintains, and an instance of a no-op transition
    // used for faster forced updates.
    pub fn new(
        id: ReplicaID,
        peer_ids: Vec<ReplicaID>,
        cluster: Box<dyn Cluster<T>>,
        state_machine: Box<S>,
        noop_transition: T,
    ) -> Replica<S, T> {
        Replica {
            state_machine: state_machine,
            cluster: cluster,
            peer_ids: peer_ids,
            id: id,
            current_term: 0,
            current_votes: None,
            state: State::Follower,
            voted_for: None,
            entries: vec![Entry {
                term: 0,
                index: 0,
                transition: noop_transition,
                state: EntryState::Queued,
            }],
            noop_transition: noop_transition,
            commit_index: 0,
            last_applied: 0,
            next_index: BTreeMap::new(),
            match_index: BTreeMap::new(),
            rng: rand::thread_rng(),
            leader_id: None,
        }
    }

    // This function starts the replica and blocks forever. Election duration is
    // randomized to avoid perpetual elections cycles. Recommended min and max
    // election timeouts are 2,500 and 3,500 milliseconds, respectively. The
    // heartbeat timeout defines how often the Leader notifies other replicas of
    // its liveness. Recommended heartbeat timeout is 1 second.
    //
    // We recommend that min election timeout should be a multiple of the
    // heartbeat timeout, depending on expected network latency.
    pub fn start(
        &mut self,
        min_election_timeout: u64,
        max_election_timeout: u64,
        heartbeat_timeout: std::time::Duration,
    ) {
        self.poll(
            (min_election_timeout, max_election_timeout),
            heartbeat_timeout,
        );
    }

    // Check if the replica is the current Leader. If yes, the Result is Ok. If
    // not, the result if Err with the ID of the current Leader, if it's known.
    pub fn is_leader(&self) -> Result<(), Option<ReplicaID>> {
        match self.state {
            State::Leader => Ok(()),
            _ => {
                if let Some(leader_id) = self.leader_id {
                    Err(Some(leader_id))
                } else {
                    Err(None)
                }
            }
        }
    }

    pub fn get_transition_status(&self, transition_id: T::TransitionID) -> Result<EntryState, ()> {
        if let Some(entry) = self
            .entries
            .iter()
            .find(|entry| entry.transition.get_id() == transition_id)
        {
            Ok(entry.state)
        } else {
            Err(())
        }
    }

    pub fn get_last_transition_statuses(&self, mut n: usize) -> Vec<(T::TransitionID, EntryState)> {
        let mut results = Vec::new();
        while n > 0 && n <= self.entries.len() {
            let index = self.entries.len() - n;
            let id = self.entries[index].transition.get_id();
            let state = self.entries[index].state;
            results.push((id, state));

            n -= 1;
        }

        results
    }

    fn broadcast_message<F>(&self, message_generator: F)
    where
        F: Fn(usize) -> Message<T>,
    {
        self.peer_ids.iter().for_each(|peer_id| {
            self.cluster
                .send(peer_id.clone(), message_generator(peer_id.clone()))
        });
    }

    fn get_entries_for_peer(&self, peer_id: ReplicaID) -> Vec<Entry<T>> {
        self.entries[self.next_index[&peer_id]..self.entries.len()].to_vec()
    }

    fn poll(&mut self, election_timeout: (u64, u64), heartbeat_timeout: std::time::Duration) {
        let mut heartbeat_timer = HeartbeatTimer::new(heartbeat_timeout);

        loop {
            let election_timeout =
                Duration::from_millis(self.rng.gen_range(election_timeout.0..=election_timeout.1));
            match self.state {
                State::Leader => self.poll_as_leader(&mut heartbeat_timer, heartbeat_timeout),
                State::Follower => self.poll_as_follower(election_timeout),
                State::Candidate => self.poll_as_candidate(election_timeout),
            }

            self.apply_ready_entries();
            self.load_new_entries();
            std::thread::sleep(std::time::Duration::from_millis(5000));
        }
    }

    fn poll_as_leader(
        &mut self,
        heartbeat_timer: &mut HeartbeatTimer,
        heartbeat_timeout: Duration,
    ) {
        if heartbeat_timer.fired() {
            self.broadcast_message(|peer_id: ReplicaID| Message::AppendEntryRequest {
                term: self.current_term,
                from_id: self.id,
                prev_log_index: self.next_index[&peer_id] - 1,
                prev_log_term: self.entries[self.next_index[&peer_id] - 1].term,
                entries: self.get_entries_for_peer(peer_id),
                commit_index: self.commit_index,
            });
            heartbeat_timer.renew();
        }

        let message = self.cluster.receive_timeout(heartbeat_timeout);
        if let Some(msg) = message {
            self.process_message_as_leader(msg)
        };
    }

    fn poll_as_follower(&mut self, election_timeout: Duration) {
        let message = self.cluster.receive_timeout(election_timeout);
        if let Some(msg) = message {
            self.process_message_as_follower(msg);
        } else {
            self.become_candidate();
        }
    }

    fn poll_as_candidate(&mut self, election_timeout: Duration) {
        let message = self.cluster.receive_timeout(election_timeout);
        if let Some(msg) = message {
            self.process_message_as_candidate(msg);
        } else {
            self.become_candidate();
        }
    }

    fn apply_ready_entries(&mut self) {
        // Move the commit index to the latest log index that has been
        // replicated on the majority of the replicas.
        if self.state == State::Leader && self.commit_index < self.entries.len() - 1 {
            let mut n = self.entries.len() - 1;
            while n > self.commit_index {
                let num_replications =
                    self.match_index.iter().fold(
                        0,
                        |acc, mtch_idx| if mtch_idx.1 >= &n { acc + 1 } else { acc },
                    );

                if num_replications * 2 >= self.peer_ids.len()
                    && self.entries[n].term == self.current_term
                {
                    self.commit_index = n;
                }
                n -= 1;
            }

            let mut last_committed = self.commit_index;
            loop {
                match self.entries[last_committed].state {
                    EntryState::Queued => {
                        self.entries[last_committed].state = EntryState::Committed;
                        last_committed -= 1;
                    }
                    _ => break,
                }
            }
        }

        // Apply entries that are behind the currently committed index.
        while self.commit_index > self.last_applied {
            self.last_applied += 1;
            self.state_machine
                .apply_transition(self.entries[self.last_applied].transition);
            self.entries[self.last_applied].state = EntryState::Applied;
        }
    }

    fn load_new_entries(&mut self) {
        for transition in self.cluster.get_transitions().iter() {
            if self.state == State::Leader {
                self.entries.push(Entry {
                    index: self.entries.len(),
                    transition: *transition,
                    term: self.current_term,
                    state: EntryState::Queued,
                });
            }
        }
    }

    fn process_message_as_leader(&mut self, message: Message<T>) {
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

    fn process_vote_request_as_follower(
        &mut self,
        from_id: ReplicaID,
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
            if self.entries[self.entries.len() - 1].index <= last_log_index
                && self.entries[self.entries.len() - 1].term <= last_log_term
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
        from_id: ReplicaID,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<Entry<T>>,
        commit_index: usize,
    ) {
        self.leader_id = Some(from_id);

        // Check that the leader's term is at least as large as ours.
        if self.current_term > term {
            self.cluster.send(
                from_id,
                Message::AppendEntryResponse {
                    from_id: self.id,
                    term: self.current_term,
                    success: false,
                    last_index: self.entries.len() - 1,
                },
            );

            return;
        // If our log doesn't contain an entry at prev_log_index with the
        // prev_log_term term, reply false.
        } else if prev_log_index >= self.entries.len()
            || self.entries[prev_log_index].term != prev_log_term
        {
            self.cluster.send(
                from_id,
                Message::AppendEntryResponse {
                    from_id: self.id,
                    term: self.current_term,
                    success: false,
                    last_index: self.entries.len() - 1,
                },
            );

            return;
        }

        for entry in entries {
            if entry.index < self.entries.len() && entry.term != self.entries[entry.index].term {
                self.entries.truncate(entry.index);
            }

            if entry.index == self.entries.len() {
                self.entries.push(entry);
            }
        }

        if commit_index > self.commit_index && self.entries.len() != 0 {
            self.commit_index = if commit_index < self.entries[self.entries.len() - 1].index {
                commit_index
            } else {
                self.entries[self.entries.len() - 1].index
            }
        }

        self.cluster.send(
            from_id,
            Message::AppendEntryResponse {
                from_id: self.id,
                term: self.current_term,
                success: true,
                last_index: self.entries.len() - 1,
            },
        );
    }

    fn process_message_as_follower(&mut self, message: Message<T>) {
        match message {
            Message::VoteRequest {
                from_id,
                term,
                last_log_index,
                last_log_term,
            } => {
                self.process_vote_request_as_follower(from_id, term, last_log_index, last_log_term)
            }
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

    fn process_message_as_candidate(&mut self, message: Message<T>) {
        match message {
            Message::AppendEntryRequest { term, from_id, .. } => {
                self.process_transition_request_as_candidate(term, from_id, message)
            }
            Message::VoteRequest { term, from_id, .. } => {
                self.process_vote_request_as_candidate(term, from_id, message)
            }
            Message::VoteResponse {
                from_id,
                term,
                vote_granted,
            } => self.process_vote_response_as_candidate(from_id, term, vote_granted),
            Message::AppendEntryResponse { .. } => { /* ignore */ }
        }
    }

    fn process_vote_response_as_candidate(
        &mut self,
        from_id: ReplicaID,
        term: usize,
        vote_granted: bool,
    ) {
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

    fn process_vote_request_as_candidate(
        &mut self,
        term: usize,
        from_id: ReplicaID,
        message: Message<T>,
    ) {
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

    fn process_transition_request_as_candidate(
        &mut self,
        term: usize,
        from_id: ReplicaID,
        message: Message<T>,
    ) {
        self.leader_id = Some(from_id);

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
                    last_index: self.entries.len() - 1,
                },
            );
        }
    }

    fn become_leader(&mut self) {
        self.leader_id = Some(self.id);
        self.state = State::Leader;
        self.current_votes = None;
        self.voted_for = None;
        self.next_index = BTreeMap::new();
        self.match_index = BTreeMap::new();
        for peer_id in &self.peer_ids {
            self.next_index.insert(peer_id.clone(), self.entries.len());
            self.match_index.insert(peer_id.clone(), 0);
        }

        // If the previous leader had some uncommitted entries that were
        // replicated to this now-leader server, this replica will not commit
        // them until its commit index advances to a log entry appended in this
        // leader's term. To carry out this operation as soon as the new leader
        // emerges, append a no-op entry. This is a neat optimization described
        // in the part 8 of the paper).
        self.entries.push(Entry {
            index: self.entries.len(),
            transition: self.noop_transition,
            term: self.current_term,
            state: EntryState::Queued,
        });
    }

    fn become_follower(&mut self, term: usize) {
        self.leader_id = None;
        self.current_term = term;
        self.state = State::Follower;
        self.current_votes = None;
        self.voted_for = None;
    }

    fn become_candidate(&mut self) {
        self.leader_id = None;
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
        self.broadcast_message(|_: usize| Message::VoteRequest {
            from_id: self.id,
            term: self.current_term,
            last_log_index: self.entries.len() - 1,
            last_log_term: self.entries[self.entries.len() - 1].term,
        });
    }
}
