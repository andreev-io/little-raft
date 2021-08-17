use crate::replica::ReplicaID;
use crate::state_machine::StateMachineTransition;

// Entry describes a user-defined transition of the distributed state machine.
// It has some associated metadata, namely the term when the entry was created
// and its index in the log.
#[derive(Clone, Debug, Copy, PartialEq, Eq, PartialOrd)]
pub struct Entry<T>
where
    T: StateMachineTransition,
{
    pub transition: T,
    pub index: usize,
    pub term: usize,
}

// Message describes messages that the replicas pass between each other to
// achieve consensus on the distributed state machine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
pub enum Message<T>
where
    T: StateMachineTransition,
{
    // AppendEntryRequest is used by Leaders to send out logs for other replicas
    // to append to their log. It also has information on what logs are ready to
    // be applied to the state machine. AppendEntryRequest is also used as a
    // heart beat message by Leaders even when no new logs need to be processed.
    AppendEntryRequest {
        from_id: ReplicaID,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<Entry<T>>,
        commit_index: usize,
    },
    // AppendEntryResponse is used by replicas to respond to AppendEntryRequest
    // messages.
    AppendEntryResponse {
        from_id: ReplicaID,
        term: usize,
        success: bool,
        last_index: usize,
    },
    // VoteRequest is used by Candidates to solicit votes for themselves.
    VoteRequest {
        from_id: ReplicaID,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    },
    // VoteResponse is used by replicas to respond to VoteRequest messages.
    VoteResponse {
        from_id: ReplicaID,
        term: usize,
        vote_granted: bool,
    },
}
