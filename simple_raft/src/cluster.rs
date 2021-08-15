use crate::message::Message;
use std::time::Duration;

// Cluster provides the means of communication of a replica with the rest of the
// cluster and the user.
pub trait Cluster<A> {
    // This function is used to deliver messages to target replicas. The
    // algorithm assumes that send can silently fail.
    fn send(&self, to_id: usize, message: Message<A>);
    // This function is used to received messages for the replicas. This
    // function must block until timeout expires or a message is received,
    // whichever comes first.
    fn receive_timeout(&self, timeout: Duration) -> Option<Message<A>>;
    // This function is used to receive actions from the user that the
    // distributed state machine needs to replicate and apply. All replicas poll
    // this function periodically but only Leaders merit the return value.
    // Non-Leaders ignore the return value of get_action.
    fn get_actions(&self) -> Vec<A>;
}
