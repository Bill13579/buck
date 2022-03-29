use std::time::{Instant, Duration};

pub struct Elapsed {
    last: Instant
}
impl Elapsed {
    pub fn new() -> Elapsed {
        Elapsed { last: Instant::now() }
    }
    pub fn elapsed(&self) -> Duration {
        Instant::now().duration_since(self.last)
    }
    pub fn update(&mut self) {
        self.last = Instant::now();
    }
}