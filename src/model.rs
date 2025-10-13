use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_tungstenite::tungstenite::Message;
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, PartialOrd, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum QueryState {
    Pending,
    Completed,
}

impl Into<String> for QueryState {
    fn into(self) -> String {
        match self {
            QueryState::Pending => "pending".to_string(),
            QueryState::Completed => "completed".to_string(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
pub(crate) struct FeasibilityRequest {
    pub(crate) id: Uuid,
    date: DateTime<Utc>,
    pub(crate) query: serde_json::Value,
    pub(crate) status: QueryState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_duration: Option<u32>,
}

impl TryInto<Message> for FeasibilityRequest {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok(Message::from(serde_json::to_string(&self)?))
    }
}
