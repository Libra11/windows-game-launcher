//! 桌面与服务端共用的白名单协议；不得加入本机路径、凭据或原始证据。
pub mod merge;
pub mod resources;
pub mod saves;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;
pub use resources::*;

pub const VERSION: u32 = 1;
pub const MAX_OPERATIONS: usize = 100;
pub const MAX_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Operation {
    pub operation_id: Uuid,
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub action: Action,
    pub base_revision: i64,
    pub depends_on_operation_id: Option<Uuid>,
    pub payload: Value,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Action { Set, Delete }

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome { Applied, Noop, Conflict, Rejected, DependencyPending }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resource {
    pub resource_type: ResourceType,
    pub resource_id: String,
    pub revision: i64,
    pub deleted: bool,
    pub payload: Value,
    pub device_id: Uuid,
    pub updated_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationResult {
    pub operation_id: Uuid,
    pub outcome: Outcome,
    pub revision: i64,
    pub sequence: i64,
    pub current: Option<Resource>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PushRequest { pub protocol_version: u32, pub operations: Vec<Operation> }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Change { pub sequence: i64, pub resource: Resource }

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangesPage {
    pub changes: Vec<Change>, pub next_cursor: i64, pub high_watermark: i64, pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPage {
    pub snapshot_id: Uuid, pub high_watermark: i64, pub resources: Vec<Resource>,
    pub next_offset: usize, pub has_more: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTokens {
    pub user_id: Uuid, pub device_id: Uuid, pub access_token: String, pub refresh_token: String,
}

pub fn digest(value: &impl Serialize) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    // serde_json 默认有序映射，先转换 Value 使结构体字段顺序不影响摘要。
    let value = serde_json::to_value(value).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(&value).map_err(|e| e.to_string())?)))
}
