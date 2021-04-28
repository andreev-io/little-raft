# Raft Implementation & CLI Visualization in Rust

## Contents
1. [Contents](https://github.com/andreev-io/Raft#contents)
2. [How does it work?](https://github.com/andreev-io/Raft#how-does-it-work)
3. [How do I use it?](https://github.com/andreev-io/Raft#how-do-i-use-it)
4. [Fun scenarios to play out?](https://github.com/andreev-io/Raft#fun-scenarios-to-play-out)
5. [How is it implemented?](https://github.com/andreev-io/Raft#how-is-it-implemented)
6. [What's next?](https://github.com/andreev-io/Raft#whats-next)

This project implements the Raft distributed consensus algorithm. It emulates
multiple state machines that communicate with each other via message-passing.
The state machine is simply a number, starting with 0, which a client may
subtract from or add to. The distributed system is guaranteed to achieve
consensus on the current number despite node failures, network partitions, and
unreliable message delivery.

## How does it work?
The Raft distributed consensus algorithm was designed to achieve correctness in
the simplest possible way. The algorithm makes the distributed system elect a
leader node, which subsequently processes client requests. To become a leader, a
node has to have more than half of the cluster vote for its candidacy. As soon
as the leader appears to be failing, the distributed system elects another
leader.

When processing a client request, the leader's responsibility is to have more
than half of the cluster replicate the request. As soon as that's done, the
leader marks the request as committed, at which point it can apply the change to
its own state machine, and then tell the other nodes to apply the request to
their state machines.

The algorithm is robust to network partitions, node outages, and message loss.
Even if some nodes in the cluster are inaccessible or have temporarily arrived
to a wrong state, the cluster as a whole always maintains the correctness of the
state machine.

To fully understand the algorithm, you have to read the original paper by Diego
Ongaro and John Ousterhout (2014): [In Search of an Understandable Consensus
Algorithm](https://raft.github.io/raft.pdf). You can supplement your reading by
looking at the Ongaro's PhD Thesis: [Consensus: Bridging Theory and
Practice](https://web.stanford.edu/~ouster/cgi-bin/papers/OngaroPhD.pdf).

## How do I use it?
You can play with the implementation and visually see how the algorithm works
right away in CLI.

First, fire up the replicas: `cargo run --release --
--message-drop-percent-probability=0 --num-replicas=5`. You will see that the
replicas move between *follower*, *candidate*, and *leader* states, and process
state changes in spite of system failures, thanks to the algorithm. 

Issue commands by appending to the `input.txt` file in the project root. Every
command must start at a new line. Command format is `<replica_id>:
<command><optional_modifier> //`. The command is run as soon as the double slash
is entered and saved at the end of the line.

Possible commands:
* `<replica_id>: Disconnect //`, for example `2: Disconnect //`.
* `<replica_id>: Connect //`, for example `2: Connect //`.
* `<replica_id>: Down //`, for example `2: Down //`.
* `<replica_id>: Up //`, for example `2: Up //`.
* `<leader_id>: Apply<signed_delta> //`, for example `3: Apply+101 //` or `4:
  Apply-11 //`. Non-leaders will ignore this message. Note: you can send this
  message to a disconnected leader.

Command descriptions:
* `Disconnect` disconnects a node from the cluster. The node preserves its full
  state, but fails to receive and send messages.
* `Connect` connects a node back to the cluster. The node starts receiving and
  sending messages.
* `Down` takes a node down. The node loses its volatile state and fails to receive and send messages.
* `Up` brings a node up. If the node was down, it starts as a *follower*, which
  is the default node state. Otherwise it just carries on.
* `Apply<signed_delta>` asks the leader to process a state change. Since the
  state in this implementation is simply a number, this command asks the cluster
  to increase or decrease that numeric value.

![Sending control messages](https://github.com/andreev-io/Raft/blob/master/screenshots/scenario.png?raw=true)

## Fun scenarios to play out?
The most fun and educative scenarios are of course those that try their hardest
to bring the cluster to a halt. Here are some ideas to explore, assuming the
number of replicas is five.

1. Disconnect the cluster leader. The rest of the cluster will elect a new
   leader, but the disconnected node will still think of itself as of a leader.
   Apply different changes to both the actual leader and the confused
   disconnected node. You will see that when the disconnected node is connected
   back, it drops its changes and accepts the true changes the rest of the
   cluster has agreed upon.

2. Disconnect a non-leader node. That node will start thinking that the leader
   went down, so it will propose its candidacy for leadership. Since no one can
   talk to the disconnected node, it will never be elected. See for yourself
   that the cluster can get far ahead in processing requests, but the
   disconnected candidate will catch up with the rest of the cluster when coming
   back.

3. Take all nodes except a single leader and a follower down. Apply a few
   changes to the leader. You will see that the leader communicates the changes
   to the follower, but they never get committed and applied to the state
   machine because they are not replicated on more than half of the cluster.
   With these shared-but-not-committed changes in progress, take the leader
   down, leaving a single follower with some uncommitted changes running. Now
   bring some other nodes back up, and you will see that the cluster will
   successfully process the uncommitted changes as soon as at least 3 nodes are
   fully live.

To take it up a notch, combine the three previous scenarios, and see for
yourself that the cluster keeps running just fine.

## How is it implemented?
The project runs a single process with multiple threads to simulate replicas &
their state machines running independently. The threads communicate with each
other via crossbeam multiple-producer single-consumer channels. An additonal
control thread periodically reads the `input.txt` file and issues control
messages to the respective replica threads. The replicas continually report back
to the control thread with status messages, which are then formatted and
refreshed in the terminal.

The messages between nodes are dropped probabilistically, with the probability
of message loss configured by the `--message-drop-percent-probability` flag. 

The replicas all set random periodic timers and expect the leader to send
heartbeat messages before the timer goes off. The variable timer value is
necessary for the nodes to propose their candidacies with some intrinsic
randomness in case of leader failure. This makes split votes less frequent and
makes it easier for the system to run successful elections.

This implementation incorporates some optimizations proposed in the paper and
the thesis. For example, each leader emits a no-op log entry (not shown in
output) upon election to make other nodes catch up on backlog changes quicker.

## What's next?
This is a solid demonstration of the algorithm. It can and should be covered
with tests, refactored, and split into a reusable Rust package that's agnostic
of the exact message passing protocols used.

As far as the demonstration goes, it should use a better interface for
control message input than the append-to-a-file approach.