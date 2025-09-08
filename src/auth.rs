use log::error;
use oauth2::basic::{BasicClient, BasicTokenResponse};
use oauth2::AuthType::RequestBody;
use oauth2::{
    reqwest, AccessToken, ClientId, ClientSecret, HttpClientError, RequestTokenError, Scope,
    TokenResponse, TokenUrl,
};
use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;

#[derive(Debug, thiserror::Error)]
pub enum TokenServiceError {
    #[error(transparent)]
    TokenError(#[from] Box<dyn Error>),
    #[error(transparent)]
    NetworkError(#[from] HttpClientError<reqwest::Error>),
}

/// Information about a token, including the token itself and when it expires
#[derive(Clone, Debug)]
pub struct TokenInfo {
    pub access_token: AccessToken,
    pub expires_at: SystemTime,
}

#[derive(Clone, Debug)]
pub struct TokenServiceConfig {
    pub token_url: String,
    pub client_id: String,
    pub client_secret: String,
}

/// A connector to the identity service that auto-renews its token when expired
#[derive(Debug, Clone)]
pub struct TokenService {
    config: TokenServiceConfig,
    token_info: Arc<Mutex<Option<TokenInfo>>>,
}

impl TokenService {
    pub fn new(config: TokenServiceConfig) -> Self {
        Self {
            config,
            token_info: Arc::new(Mutex::new(None)),
        }
    }

    async fn initialize_service(&self) -> Result<TokenInfo, TokenServiceError> {
        let token = self.perform_login().await?;

        let expires_at = SystemTime::now()
            + Duration::from_secs(
                token
                    .expires_in()
                    .ok_or_else(|| {
                        TokenServiceError::TokenError("Token has no duration".to_string().into())
                    })?
                    .as_secs(),
            );

        Ok(TokenInfo {
            access_token: token.access_token().clone(),
            expires_at,
        })
    }

    async fn perform_login(&self) -> Result<BasicTokenResponse, TokenServiceError> {
        let client = BasicClient::new(ClientId::new(self.config.client_id.clone()))
            .set_auth_type(RequestBody)
            .set_client_secret(ClientSecret::new(self.config.client_secret.clone()))
            .set_token_uri(
                TokenUrl::new(self.config.token_url.clone()).expect("Token URL should be valid"),
            );

        let http_client = reqwest::ClientBuilder::new()
            // Following redirects opens the client up to SSRF vulnerabilities.
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Client should build");

        let result = client
            .exchange_client_credentials()
            .add_scope(Scope::new("profile".to_string()))
            .request_async(&http_client)
            .await;

        result.map_err(|e| match e {
            RequestTokenError::Request(e) => TokenServiceError::NetworkError(e),
            RequestTokenError::ServerResponse(e) => {
                TokenServiceError::TokenError(e.to_string().into())
            }
            _ => {
                error!("Unexpected error: {:?}", e);
                TokenServiceError::TokenError("Unexpected error".to_string().into())
            }
        })
    }

    pub async fn get_token(&self) -> Result<AccessToken, TokenServiceError> {
        let mut token_info = self.token_info.lock().await;

        if token_info.is_none() || token_info.as_ref().unwrap().expires_at < SystemTime::now() {
            let new_token_info = self.initialize_service().await?;
            *token_info = Some(new_token_info);
        }

        Ok(token_info.as_ref().unwrap().access_token.clone())
    }
}
