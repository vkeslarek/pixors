#[derive(Debug, Clone)]
pub struct Routed<T> {
    pub port: u16,
    pub payload: T,
}
