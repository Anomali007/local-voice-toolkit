pub mod frontmost_app;
pub mod paste;
pub mod selected_text;

pub use frontmost_app::{get_frontmost_app, FrontmostAppInfo};
pub use paste::paste_text;
pub use selected_text::get_selected_text;
