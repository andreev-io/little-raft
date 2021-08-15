use crate::replica::Action;

#[derive(Clone, Debug)]
pub enum Message<A> {
    AppendEntryRequest {
        from_id: usize,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<Action<A>>,
        commit_index: usize,
    },
    AppendEntryResponse {
        from_id: usize,
        term: usize,
        success: bool,
        last_index: usize,
    },
    VoteRequest {
        from_id: usize,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    },
    VoteResponse {
        from_id: usize,
        term: usize,
        vote_granted: bool,
    },
}
