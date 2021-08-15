use crate::message::Message;

pub trait Cluster<A>: std::fmt::Debug {
    fn send(&self, to_id: usize, message: Message<A>);
    fn receive_timeout(&self, timeout: std::time::Duration) -> Option<Message<A>>;
    fn get_action(&self) -> Option<A>;
}
