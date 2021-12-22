use crate::replica::ReplicaID;
use crate::state_machine::{StateMachineTransition};

/// LogEntry is a state machine transition along with some metadata needed for
/// Raft.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd)]
pub struct LogEntry<T>
where
    T: StateMachineTransition,
{
    pub transition: T,
    pub index: usize,
    pub term: usize,
}

/// Message describes messages that the replicas pass between each other to
/// achieve consensus on the distributed state machine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub enum Message<T, D>
where
    T: StateMachineTransition,
    D: Clone,
{
    /// AppendEntryRequest is used by the Leader to send out logs for other
    /// replicas to append to their log. It also has information on what logs
    /// are ready to be applied to the state machine. AppendEntryRequest is also
    /// used as a heartbeat message by the Leader even when no new logs need to
    /// be processed.
    AppendEntryRequest {
        from_id: ReplicaID,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<LogEntry<T>>,
        commit_index: usize,
    },

    /// AppendEntryResponse is used by replicas to respond to AppendEntryRequest
    /// messages.
    AppendEntryResponse {
        from_id: ReplicaID,
        term: usize,
        success: bool,
        last_index: usize,
        mismatch_index: Option<usize>,
    },

    /// VoteRequest is used by Candidates to solicit votes for themselves.
    VoteRequest {
        from_id: ReplicaID,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    },

    /// VoteResponse is used by replicas to respond to VoteRequest messages.
    VoteResponse {
        from_id: ReplicaID,
        term: usize,
        vote_granted: bool,
    },

    InstallSnapshotRequest {
        from_id: ReplicaID,
        term: usize,
        last_included_index: usize,
        last_included_term: usize,
        offset: usize,
        data: D,
        done: bool,
    },

    InstallSnapshotResponse {
        from_id: ReplicaID,
        term: usize,
        last_included_index: usize,
    },
}
