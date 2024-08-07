use serde::{Deserialize, Serialize};

use crate::globals::PizzaConfig;

#[derive(Serialize, Deserialize)]
pub struct ArchiveEntry {
    timestamp: i64, // Might want to replace this with something else
    config: PizzaConfig,
    // TODO more?
}
