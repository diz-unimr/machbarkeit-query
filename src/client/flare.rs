use crate::config::Feasibility;
use crate::model::{FeasibilityRequest, QueryState};
use anyhow::Error;
use http::header::CONTENT_TYPE;
use log::info;
use reqwest::Client;

pub(crate) struct FlareClient {
    base_url: String,
    client: Client,
}

impl FlareClient {
    pub(crate) fn new(config: &Feasibility) -> Result<Self, Error> {
        Ok(FlareClient {
            // todo
            base_url: config.base_url.clone(),
            client: Client::builder().build()?,
        })
    }
    pub(crate) async fn execute(
        &self,
        request: FeasibilityRequest,
    ) -> Result<FeasibilityRequest, Error> {
        info!("Sending request id={} to {}", &request.id, &self.base_url);
        let payload = serde_json::to_string(&request.query)?;
        let response = self
            .client
            .post(self.base_url.clone())
            .body(payload.to_owned())
            .header(CONTENT_TYPE, "application/sq+json")
            .send()
            .await?;

        let request = request.result(
            QueryState::Completed,
            response.status().as_u16(),
            response.text().await?,
        );
        Ok(request)
    }
}
