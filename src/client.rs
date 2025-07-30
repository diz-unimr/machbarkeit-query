use crate::config::Server;
use crate::model::{FeasibilityRequest, QueryState};
use log::debug;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct RestClient {
    client: ClientWithMiddleware,
    url: String,
}

impl RestClient {
    pub(crate) fn new(config: &Server) -> Result<Self, anyhow::Error> {
        // default headers
        let mut headers = HeaderMap::new();
        headers.insert(
            CONTENT_TYPE,
            HeaderValue::from_static("application/fhir+json"),
        );
        // set auth header as default
        if let Some(auth) = config.auth.as_ref().and_then(|a| a.basic.as_ref()) {
            if let (Some(user), Some(password)) = (auth.user.clone(), auth.password.clone()) {
                // auth header
                let auth_value = create_auth_header(user, Some(password));
                headers.insert(AUTHORIZATION, auth_value);
            }
        }

        // retry
        let retry = ExponentialBackoff::builder()
            .retry_bounds(
                Duration::from_secs(config.retry.wait),
                Duration::from_secs(config.retry.max_wait),
            )
            .build_with_max_retries(config.retry.count);

        // client with retry middleware
        let client = ClientBuilder::new(
            Client::builder()
                .default_headers(headers.clone())
                .timeout(Duration::from_secs(config.retry.timeout))
                .build()?,
        )
        .with(RetryTransientMiddleware::new_with_policy(retry))
        .build();

        Ok(RestClient {
            client,
            url: config.base_url.clone(),
        })
    }

    pub(crate) async fn execute(
        self,
        request: &mut FeasibilityRequest,
    ) -> Result<&FeasibilityRequest, anyhow::Error> {
        debug!("Sending bundle to: {}", self.url);
        let payload = serde_json::to_string(&request.query)?;
        let response = self
            .client
            .post(self.url)
            .body(payload.to_owned())
            .header(CONTENT_TYPE, "application/sq+json")
            .send()
            .await?;

        request.status = QueryState::Completed;
        request.result_code = Some(response.status().as_u16());
        request.result_body = Some(response.text().await?);
        Ok(request)
    }
}

fn create_auth_header(user: String, password: Option<String>) -> HeaderValue {
    let builder = Client::new()
        .get("http://localhost")
        .basic_auth(user, password);

    builder
        .build()
        .unwrap()
        .headers()
        .get(AUTHORIZATION)
        .unwrap()
        .clone()
}
