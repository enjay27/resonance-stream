mod app_updater;
mod gist;
pub mod model;
pub mod server;

pub use self::app_updater::*;
pub use self::gist::*;
pub use self::model::*;
pub use self::server::*;
use serde::Serialize;

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
