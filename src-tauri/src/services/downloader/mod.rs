pub mod model;
pub mod server;
pub mod dictionary;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Clone)]
pub struct FolderStatus {
    pub exists: bool,
    pub path: String,
}

#[derive(Serialize, Clone)]
pub struct ProgressPayload {
    pub current_file: String,
    pub percent: u8,
    pub total_percent: u8,
}

pub use self::model::*;
pub use self::server::*;
pub use self::dictionary::*;