use crossbeam::channel::{bounded, Receiver};
use std::{thread, time::Duration};

pub struct Timer {
    rx: Receiver<()>,
    timeout: Duration,
}

// Timer fires after the specified duration. The timer can be renewed.
impl Timer {
    pub fn new(timeout: Duration) -> Timer {
        Timer {
            timeout,
            rx: Timer::get_timeout_channel(timeout),
        }
    }

    pub fn renew(&mut self) {
        self.rx = Timer::get_timeout_channel(self.timeout);
    }

    pub fn get_rx(&self) -> &Receiver<()> {
        &self.rx
    }

    fn get_timeout_channel(timeout: Duration) -> Receiver<()> {
        let (tx, rx) = bounded(1);
        thread::spawn(move || {
            thread::sleep(timeout);
            let _ = tx.send(());
        });

        rx
    }
}
