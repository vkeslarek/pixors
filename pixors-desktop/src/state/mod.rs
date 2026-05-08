#![allow(dead_code)]

pub mod tab;
pub mod editor;
pub mod history;

pub use tab::{Tab, TabId, TabSource, TabView};
pub use editor::EditorState;
