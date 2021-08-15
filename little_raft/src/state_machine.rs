pub trait StateMachineTransition: Copy + Clone {
    type TransitionID: Eq;
    fn get_id(&self) -> Self::TransitionID;
}

// StateMachine describes a user-defined state machine that is replicated across
// the cluster.
pub trait StateMachine<T>
where
    T: StateMachineTransition,
{
    // When apply_transition is called, the local state machine must apply the
    // specified transition.
    fn apply_transition(&mut self, transition: T);
}
