pub trait StateMachine<A> {
    fn new() -> Self;
    fn apply_action(&mut self, action: A);
}
