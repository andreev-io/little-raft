//! This crate is a small but full-featured implementation of the Raft
//! distributed consensus protocol. By using this library, you can run a
//! replicated state machine in your own cluster. The cluster could be comprised
//! of dozens of physical servers in different parts of the world or of two
//! threads on a single CPU.
//!
//! The goal of this library is to provide a generic implementation of the
//! algorithm that the library user can leverage in their own way. It is
//! entirely up to the user how to configure the Raft cluster, how to ensure
//! communication between the nodes, how to process client's messages, how to do
//! service discovery, and what kind of state machine to replicate.
//!
//! The implementation is kept as simple as possible on purpose, with the entire
//! library code base fitting in under 1,000 lines of code.
pub mod cluster;
pub mod message;
pub mod replica;
pub mod state_machine;
mod timer;
