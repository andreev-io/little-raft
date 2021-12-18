use bytes::Bytes;
use std::fmt::Debug;

/// TransitionState describes the state of a particular transition.
#[derive(Clone, Debug, PartialEq)]
pub enum TransitionState {
    /// Queued transitions have been received from the user but have not been
    /// processed yet. They are in the queue.
    Queued,

    /// Committed transitions have not yet been applied to the state machine but
    /// have already been replicated across the cluster such that they are
    /// guaranteed to be present in the log of all future cluster leaders.
    Committed,

    /// Applied transitions have been replicated across the cluster and have
    /// been applied to the local state machine.
    Applied,

    /// Abandoned transitions have been ignored by the replica.
    Abandoned(TransitionAbandonedReason),
}

/// TransitionAbandonedReason explains why a particular transition has been
/// abandoned by the replica.
#[derive(Clone, Debug, PartialEq)]
pub enum TransitionAbandonedReason {
    /// NotLeader transitions have been abandoned because the replica is not the
    /// cluster leader.
    NotLeader,

    // ConflictWithLeader uncommitted transitions are abandoned because they
    // don't match the consensus achieved by the majority of the cluster.
    ConflictWithLeader,
}

/// StateMachineTransition describes a user-defined transition that can be
/// applied to the state machine replicated by Raft.
pub trait StateMachineTransition: Clone + Debug {
    /// TransitionID is used to identify the transition.
    type TransitionID: Eq;

    /// get_id is used by the Replica to identify the transition to be able to
    /// call register_transition_state.
    fn get_id(&self) -> Self::TransitionID;
}

/// Snapshot is an object used for log compaction. The user can use snapshots to
/// represent StateMachine state at a particular point. This will let the
/// Replica start from a saved state or perform log compaction before the log
/// sequence starts taking up too much memory.
#[derive(Clone)]
pub struct Snapshot {
    pub last_included_index: usize,
    pub last_included_term: usize,
    pub data: Bytes,
}

/// StateMachine describes a user-defined state machine that is replicated
/// across the cluster. Raft can replicate whatever distributed state machine
/// can implement this trait.
pub trait StateMachine<T>
where
    T: StateMachineTransition,
{
    /// This is a hook that the local Replica will call each time the state of a
    /// particular transition changes. It is up to the user what to do with that
    /// information.
    fn register_transition_state(&mut self, transition_id: T::TransitionID, state: TransitionState);

    /// When a particular transition is ready to be applied, the Replica will
    /// call apply_transition to apply said transition to the local state
    /// machine.
    fn apply_transition(&mut self, transition: T);

    /// This function is used to receive transitions from the user that need to
    /// be applied to the replicated state machine. Note that only the Leader
    /// Replica processes transitions and only when notified via the
    /// recv_transition channel. All other Replicas poll for transitions and
    /// discard them. get_pending_transitions must not return the same
    /// transition twice.
    fn get_pending_transitions(&mut self) -> Vec<T>;

    /// Replica calls get_snapshot once upon startup. If the Replica and the
    /// associated StateMachine should start from a certain checkpoint
    /// previously saved with a call to create_snapshot or set_snapshot, this
    /// function should return Some(snapshot). Otherwise it can return None. If
    /// None is returned, the Replica can still recover its state from other
    /// nodes in the cluster, but it might take longer to do so than if it
    /// recovered from a previously snapshotted value.
    ///
    /// Little Raft will take care of loading the Snapshot into the Replica and
    /// achieving consensus provided snapshot.last_included_index and
    /// snapshot.last_included_term are truthful. However, it is up to the user
    /// to put the StateMachine into the right state before returning from
    /// load_snapshot().
    fn get_snapshot(&mut self) -> Option<Snapshot>;

    /// create_snapshot is periodically called by the Replica if log compaction
    /// is enabled by setting snapshot_delta > 0. The implementation MUST create
    /// a snapshot object with truthful values of index and term.
    ///
    /// If the Replica should use this snapshot as a checkpoint upon restart,
    /// the implementation MUST save the created snapshot object to permanent
    /// storage and return it with get_snapshot after restart.
    fn create_snapshot(
        &mut self,
        last_included_index: usize,
        last_included_term: usize,
    ) -> Snapshot;

    /// When a Replica receives a snapshot from another Replica, set_snapshot is
    /// called. The StateMachine MUST then load its state from the provided
    /// snapshot and potentially save said snapshot to persistent storage, same
    /// way it is done in create_snapshot.
    fn set_snapshot(&mut self, snapshot: Snapshot);
}
