use std::collections::VecDeque;

pub struct MessageBuffer<T> {
    buf: VecDeque<T>,
}

impl<T> MessageBuffer<T> {
    pub fn with_capacity(n: u16) -> Self {
        MessageBuffer {
            buf: VecDeque::with_capacity(n as usize),
        }
    }
}
