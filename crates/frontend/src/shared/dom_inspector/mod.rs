pub mod page;
pub mod tree_builder;
pub mod tree_view;
pub mod validator;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DomNode {
    pub tag_name: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    pub data_attributes: HashMap<String, String>,
    pub button_text: Option<String>,
    pub children: Vec<DomNode>,
    pub depth: usize,
}

const DOM_SNAPSHOT_KEY: &str = "dom_inspector_snapshot";

pub fn get_dom_snapshot() -> Option<DomNode> {
    web_sys::window()?
        .local_storage()
        .ok()??
        .get_item(DOM_SNAPSHOT_KEY)
        .ok()?
        .and_then(|json| serde_json::from_str(&json).ok())
}

pub fn set_dom_snapshot(tree: &DomNode) {
    if let Some(storage) = web_sys::window()
        .and_then(|w| w.local_storage().ok())
        .flatten()
    {
        if let Ok(json) = serde_json::to_string(tree) {
            let _ = storage.set_item(DOM_SNAPSHOT_KEY, &json);
        }
    }
}
