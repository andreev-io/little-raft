use std::fmt::Debug;

// State of a particular transition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TransitionState {
    // Transition being queued means that the replica is aware of it and is
    // replicating it across the cluster.
    Queued,
    // Transition being committed means that the entry is guaranteed to be in
    // the log of all future leaders in the cluster.
    Committed,
    // Entry being applied means that it has been applied to the state machine.
    Applied,
}

pub trait StateMachineTransition: Copy + Clone + Debug {
    type TransitionID: Eq;
    fn get_id(&self) -> Self::TransitionID;
}

// StateMachine describes a user-defined state machine that is replicated across
// the cluster.
pub trait StateMachine<T>
where
    T: StateMachineTransition,
{
    fn register_transition_state(&mut self, transition_id: T::TransitionID, state: TransitionState);

    // When apply_transition is called, the local state machine must apply the
    // specified transition.
    fn apply_transition(&mut self, transition: T);
}
