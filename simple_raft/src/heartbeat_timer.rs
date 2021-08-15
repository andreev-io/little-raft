use crossbeam::channel::{bounded, Receiver};
use std::{thread, time::Duration};

pub struct HeartbeatTimer {
    timeout: Duration,
    rx: Receiver<()>,
}

// HeartbeatTimer fires after the specified duration. The timer can be renewed.
impl HeartbeatTimer {
    pub fn new(timeout: Duration) -> HeartbeatTimer {
        let (tx, rx) = bounded(1);

        thread::spawn(move || {
            thread::sleep(timeout);
            tx.send(()).unwrap();
        });

        HeartbeatTimer {
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
