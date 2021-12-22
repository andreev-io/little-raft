use crate::{message::Message, replica::ReplicaID, state_machine::{StateMachineTransition}};

/// Cluster is used for the local Raft Replica to communicate with the rest of
/// the Raft cluster. It is up to the user how to abstract that communication.
/// The Cluster trait also contains hooks which the Replica will use to inform
/// the crate user of state changes.
pub trait Cluster<T, D>
where
    T: StateMachineTransition,
    D: Clone,
{
    /// This function is used to deliver messages to target Replicas. The
    /// Replica will provide the to_id of the other Replica it's trying to send
    /// its message to and provide the message itself. The send_message
    /// implementation must not block but is allowed to silently fail -- Raft
    /// exists to achieve consensus in spite of failures, after all.
    fn send_message(&mut self, to_id: usize, message: Message<T, D>);

    /// This function is used by the Replica to receive pending messages from
    /// the cluster. The receive_messages implementation must not block and must
    /// not return the same message more than once. Note that receive_messages
    /// is only called when the Replica is notified via the recv_msg channel.
    fn receive_messages(&mut self) -> Vec<Message<T, D>>;

    /// By returning true from halt you can signal to the Replica that it should
    /// stop running.
    fn halt(&self) -> bool;

    /// This function is a hook that the Replica uses to inform the user of the
    /// Leader change. The leader_id is an Option<usize> because the Leader
    /// might be unknown for a period of time. Remember that only Leaders can
    /// process transitions submitted by the Raft users, so the leader_id can be
    /// used to redirect the requests from non-Leader nodes to the Leader node.
    fn register_leader(&mut self, leader_id: Option<ReplicaID>);
}
