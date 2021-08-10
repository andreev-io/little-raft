use crossbeam::channel::Sender;
use crate::replica::Message;
use rand::Rng;


#[derive(Clone, Debug)]
pub struct Peer {
    pub id: usize,
    tx: Sender<Message>,
    drop_prob: usize,
}

impl Peer {
    pub fn new(id: usize, tx: Sender<Message>, percent_probability_message_drop: usize) -> Peer {
        Peer {
            id: id,
            tx: tx,
            drop_prob: percent_probability_message_drop,
        }
    }

    pub fn send(&self, message: Message) {
        let mut rng = rand::thread_rng();
        let val = rng.gen_range(0..=100);
        if val >= self.drop_prob {
            self.tx.send(message).unwrap();
        }
    }
}