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

impl From<QueryState> for String {
    fn from(state: QueryState) -> Self {
        match state {
            QueryState::Pending => "pending".to_string(),
            QueryState::Completed => "completed".to_string(),
        }
    }
}

#[derive(Deserialize, Serialize, Debug, PartialEq, Clone)]
pub(crate) struct FeasibilityRequest {
    pub(crate) id: Uuid,
    pub(crate) date: DateTime<Utc>,
    pub(crate) query: serde_json::Value,
    pub(crate) status: QueryState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) result_duration: Option<u32>,
}

impl FeasibilityRequest {
    pub(crate) fn result(
        mut self,
        status: QueryState,
        result_code: u16,
        result_body: String,
    ) -> Self {
        self.status = status;
        self.result_code = Some(result_code);
        self.result_body = Some(result_body);

        self
    }
}

impl TryInto<Message> for FeasibilityRequest {
    type Error = anyhow::Error;

    fn try_into(self) -> Result<Message, Self::Error> {
        Ok(Message::from(serde_json::to_string(&self)?))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::QueryState;

    #[test]
    fn into_query_state_test() {
        let pending: String = QueryState::Pending.into();
        let completed: String = QueryState::Completed.into();
        assert_eq!(pending, "pending");
        assert_eq!(completed, "completed");
    }
}
