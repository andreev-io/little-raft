# Little Raft
The lightest distributed consensus library. Run your own replicated state machine! :heart:

## Installing
Simply import the crate. In your `Cargo.toml`, add
```
[dependencies]
little_raft = "0.2.0"
```

## Using
To start running Little Raft, you only need to do three things.
1. Implement the StateMachine that you want your cluster to maintain. Little Raft will take care of replicating this machine across the cluster and achieving consensus on its state.
```rust
/// StateMachine describes a user-defined state machine that is replicated
/// across the cluster. Raft can Replica whatever distributed state machine can
/// implement this trait.
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
    /// be applied to the replicated state machine. Note that while all Replicas
    /// poll get_pending_transitions periodically, only the Leader Replica
    /// actually processes them. All other Replicas discard pending transitions.
    /// get_pending_transitions must not return the same transition twice.
    fn get_pending_transitions(&mut self) -> Vec<T>;
}
```

2. Implement the Cluster abstraction so that the local Replica can communicate with other nodes.
```rust
/// Cluster is used for the local Raft Replica to communicate with the rest of
/// the Raft cluster. It is up to the user how to abstract that communication.
/// The Cluster trait also contains hooks which the Replica will use to inform
/// the crate user of state changes.
pub trait Cluster<T>
where
    T: StateMachineTransition,
{
    /// This function is used to deliver messages to target Replicas. The
    /// Replica will provide the to_id of the other Replica it's trying to send
    /// its message to and provide the message itself. The send_message
    /// implementation must not block but is allowed silently fail -- Raft
    /// exists to achieve consensus in spite of failures, after all.
    fn send_message(&mut self, to_id: usize, message: Message<T>);

    /// This function is used by the Replica to receive pending messages from
    /// the cluster. The receive_messages implementation must not block and must
    /// not return the same message more than once.
    fn receive_messages(&mut self) -> Vec<Message<T>>;

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
```
3. Start your replica!
```rust
    /// Create a new Replica.
    ///
    /// id is the ID of this Replica within the cluster.
    ///
    /// peer_ids is a vector of IDs of all other Replicas in the cluster.
    ///
    /// cluster represents the abstraction the Replica uses to talk with other
    /// Replicas.
    ///
    /// state_machine is the state machine that Raft maintains.
    ///
    /// noop_transition is a transition that can be applied to the state machine
    /// multiple times with no effect.
    ///
    /// heartbeat_timeout defines how often the Leader Replica sends out
    /// heartbeat messages.
    ///
    /// election_timeout_range defines the election timeout interval. If the
    /// Replica gets no messages from the Leader before the timeout, it
    /// initiates an election.
    ///
    /// In practice, pick election_timeout_range to be 2-3x the value of
    /// heartbeat_timeout, depending on your particular use-case network latency
    /// and responsiveness needs. An election_timeout_range / heartbeat_timeout
    /// ratio that's too low might cause unwarranted re-elections in the
    /// cluster.
    pub fn new(
        id: ReplicaID,
        peer_ids: Vec<ReplicaID>,
        cluster: Arc<Mutex<C>>,
        state_machine: Arc<Mutex<S>>,
        noop_transition: T,
        heartbeat_timeout: Duration,
        election_timeout_range: (Duration, Duration),
    ) -> Replica<S, T, C>;

    /// This function starts the Replica and blocks forever.
    ///
    /// recv_msg is a channel on which the user must notify the Replica whenever
    /// new messages from the Cluster are available. The Replica will not poll
    /// for messages from the Cluster unless notified through recv_msg.
    ///
    /// recv_transition is a channel on which the user must notify the Replica
    /// whenever new transitions to be processed for the StateMachine are
    /// available. The Replica will not poll for pending transitions for the
    /// StateMachine unless notified through recv_transition.
    pub fn start(&mut self, recv_msg: Receiver<()>, recv_transition: Receiver<()>);
```


With that, you're good to go. We are working on examples, but for now you can look at the `little_raft/tests` directory and at the documentation at [https://docs.rs/little_raft/0.1.3/little_raft/](https://docs.rs/little_raft/0.1.3/little_raft/). We're working on adding more tests.


## Testing
Run `cargo test`.

## Contributing
Contributions are very welcome! Do remember that one of the goals of this library is to be as small and simple as possible. Let's keep the code in `little_raft/src` **under 1,000 lines**. PRs breaking this rule will be declined.
```bash
> cloc little_raft/src
       6 text files.
       6 unique files.                              
       0 files ignored.

github.com/AlDanial/cloc v 1.90  T=0.02 s (369.2 files/s, 56185.0 lines/s)
-------------------------------------------------------------------------------
Language                     files          blank        comment           code
-------------------------------------------------------------------------------
Rust                             6             82            199            632
-------------------------------------------------------------------------------
SUM:                             6             82            199            632
-------------------------------------------------------------------------------
```

You are welcome to pick up and work on any of the issues open for this project. Or you can submit new issues if anything comes up from your experience using this library.