use crossbeam::channel::{bounded, Receiver, Sender};
use std::{thread, time::Duration};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    Follower,
    Candidate,
    Leader,
    Dead,
}

pub enum ControlMessage {
    Up,
    Down,
    Apply(i32),
}

#[derive(Clone, Debug)]
pub enum Message {
    AppendEntryRequest {
        from_id: usize,
        term: usize,
        prev_log_index: usize,
        prev_log_term: usize,
        entries: Vec<Log>,
        commit_index: usize,
    },
    AppendEntryResponse {
        from_id: usize,
        term: usize,
        success: bool,
        last_index: usize,
    },
    RequestVoteRequest {
        from_id: usize,
        term: usize,
        last_log_index: usize,
        last_log_term: usize,
    },
    RequestVoteResponse {
        from_id: usize,
        term: usize,
        vote_granted: bool,
    },
}

#[derive(Clone, Debug)]
pub struct Peer {
    pub id: usize,
    tx: Sender<Message>,
}

impl Peer {
    pub fn new(id: usize, tx: Sender<Message>) -> Peer {
        Peer { id: id, tx: tx }
    }

    pub fn send(&self, message: Message) {
        self.tx.send(message).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct Log {
    pub index: usize,
    pub delta: i32,
    pub term: usize,
}

pub struct LeaderTimer {
    timeout: Duration,
    rx: Receiver<()>,
}

impl LeaderTimer {
    pub fn new(timeout: Duration) -> LeaderTimer {
        let (tx, rx) = bounded(1);

        thread::spawn(move || {
            thread::sleep(timeout);
            tx.send(()).unwrap();
        });

        LeaderTimer {
            timeout: timeout,
            rx: rx,
        }
    }

    pub fn renew(&mut self) {
        let (tx, rx) = bounded(1);
        let timeout = self.timeout;
        thread::spawn(move || {
            thread::sleep(timeout);
            tx.send(()).unwrap();
        });

        self.rx = rx;
    }

    pub fn fired(&mut self) -> bool {
        match self.rx.try_recv() {
            Ok(_) => true,
            Err(_) => false,
        }
    }
}
