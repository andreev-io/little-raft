// Action describes a user-defined transition of the distributed state machine.
// It has some associated metadata, namely the term when the action was created
// and its index in the log.
#[derive(Clone)]
pub struct Action<A> {
    pub action: A,
    pub index: usize,
    pub term: usize,
}

// Message describes messages that the replicas pass between each other to
// achieve consensus on the distributed state machine.
pub enum Message<A> {
    // ActionRequest is used by Leaders to send out actions for other nodes to
    // append to their log. It also has information on what actions are ready to
    // be applied to the state machine. ActionRequest is also used as a heart
    // beat message by Leaders even when no new actions need to be processed.
    ActionRequest {
        from_id: usize,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        actions: Vec<Action<A>>,
        commit_index: usize,
    },
    // ActionResponse is used by replicas to respond to ActionRequest messages.
    ActionResponse {
        from_id: usize,
        term: usize,
        success: bool,
        last_index: usize,
    },
    // VoteRequest is used by Candidates to solicit votes for themselves.
    VoteRequest {
        from_id: usize,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    },
    // VoteResponse is used by replicas to respond to VoteRequest messages.
    VoteResponse {
        from_id: usize,
        term: usize,
        vote_granted: bool,
    },
}
