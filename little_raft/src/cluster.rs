use crate::{message::Message, state_machine::StateMachineTransition};

// Cluster provides the means of communication of a replica with the rest of the
// cluster and the user.
pub trait Cluster<T>
where
    T: StateMachineTransition,
{
    // This function is used to deliver messages to target replicas. The
    // algorithm assumes that send can silently fail.
    fn send(&mut self, to_id: usize, message: Message<T>);
    // This function is used to received messages for the replicas. If there are
    // no messages pending, the function should return immediately.
    fn receive(&mut self) -> Vec<Message<T>>;
    // This function is used to receive actions from the user that the
    // distributed state machine needs to replicate and apply. All replicas poll
    // this function periodically but only Leaders merit the return value.
    // Non-Leaders ignore the return value of get_action.
    fn get_pending_transitions(&mut self) -> Vec<T>;

    fn halt(&self) -> bool;
}
