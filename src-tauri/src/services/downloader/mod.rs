pub mod model;
pub mod server;
mod gist;
mod app_updater;

use serde::Serialize;
pub use self::model::*;
pub use self::server::*;
pub use self::gist::*;
pub use self::app_updater::*;

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