use std::time::Duration;

#[derive(Clone, Copy, Debug)]
pub struct ForkWaits {
    pub answer: Duration,
    pub resume: Duration,
    pub settle: Duration,
    // Herdr sends no event when an agent's session changes.
    pub resume_poll: Duration,
    pub resubscribe: Duration,
    // Claude Code writes the end of its turn to its transcript after it tells herdr-core
    // that it is idle.
    pub transcript: Duration,
}

impl Default for ForkWaits {
    fn default() -> Self {
        Self {
            answer: Duration::from_secs(10),
            resume: Duration::from_secs(20),
            settle: Duration::from_secs(20),
            resume_poll: Duration::from_millis(100),
            resubscribe: Duration::from_secs(1),
            transcript: Duration::from_secs(5),
        }
    }
}
