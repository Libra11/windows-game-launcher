mod commands;
pub(crate) mod store;
#[cfg(test)] mod tests;
pub(crate) use commands::*;

use serde::{Deserialize, Serialize};
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Item { pub id: String, pub name: String }
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Collection { pub id: String, pub name: String, pub position: i64 }
#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Organization {
    pub tags: Vec<Item>, pub collections: Vec<Collection>,
    pub game_tags: Vec<(String,String)>, pub game_collections: Vec<(String,String)>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all="camelCase", deny_unknown_fields)]
pub struct Changes {
    pub game_ids: Vec<String>, pub add_tag_ids: Vec<String>, pub remove_tag_ids: Vec<String>,
    pub add_collection_ids: Vec<String>, pub remove_collection_ids: Vec<String>,
}
