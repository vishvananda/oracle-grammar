//! Physical actions whose paid-cost receipts can be inspected.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PaymentAction {
    Sacrifice,
    Discard,
    Tap,
    Untap,
    Exile,
    RemoveCounters,
    Enter,
    Reveal,
    Draw,
    Mill,
    /// The cost picked one object and left it where it was.
    Choose,
    Return,
}
