use crossbeam::channel::{bounded, Receiver};
use std::{thread, time::Duration};

pub struct Timer {
    rx: Receiver<()>,
    timeout: Duration,
}

// Timer fires after the specified duration. The timer can be renewed.
impl Timer {
    pub fn new(timeout: Duration) -> Timer {
        let (tx, rx) = bounded(1);
        thread::spawn(move || {
            thread::sleep(timeout);
            let _ = tx.send(());
        });

        Timer {
            timeout: timeout,
            rx: rx,
        }
    }

    pub fn renew(&mut self) {
        let (tx, rx) = bounded(1);
        let timeout = self.timeout;
        thread::spawn(move || {
            thread::sleep(timeout);
            let _ = tx.send(());
        });

        self.rx = rx;
    }

    pub fn get_rx(&self) -> &Receiver<()> {
        &self.rx
    }
}
