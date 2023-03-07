use derive_more::{Display, From, Into};
use serde::Deserialize;
use std::num::ParseIntError;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Arc;

#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct Interruptor(Arc<AtomicBool>);

impl Interruptor {
    pub fn new() -> Self {
        Interruptor(Arc::new(AtomicBool::new(false)))
    }

    pub fn set(&self) {
        self.0.store(true, SeqCst);
    }

    pub fn is_set(&self) -> bool {
        self.0.load(SeqCst)
    }
}

impl Default for Interruptor {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(
    Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Debug, Deserialize, From, Into, Display,
)]
#[repr(transparent)]
pub struct RetryDurationUs(pub u64);

impl Default for RetryDurationUs {
    fn default() -> Self {
        // 100ms
        RetryDurationUs(100000)
    }
}

impl FromStr for RetryDurationUs {
    type Err = ParseIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(RetryDurationUs(s.trim().parse::<u64>()?))
    }
}
