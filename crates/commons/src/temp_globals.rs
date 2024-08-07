use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct WebsiteStatus {
    pub status: LockedStatus,
    pub announcement: String,
}

#[derive(Serialize, Deserialize)]
pub enum LockedStatus {
    Open,
    Locked,
}
