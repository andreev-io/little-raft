// StateMachine describes a user-defined state machine that is replicated across
// the cluster.
pub trait StateMachine<A> {
    // When apply_action is called, the local state machine must apply the
    // specified action.
    fn apply_action(&mut self, action: A);
}
