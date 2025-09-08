use config::{Config, ConfigError, Environment, File};
use serde_derive::Deserialize;

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct App {
    pub(crate) log_level: String,
}

#[derive(Default, Deserialize, Clone)]
pub(crate) struct AppConfig {
    pub(crate) app: App,
    pub(crate) feasibility: Server,
    pub(crate) broker: Broker,
}

#[derive(Default, Deserialize, Clone)]
pub(crate) struct Broker {
    pub(crate) url: String,
    pub(crate) auth: Option<Auth>,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Server {
    pub(crate) base_url: String,
    pub(crate) auth: Option<Auth>,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Auth {
    pub(crate) basic: Option<Basic>,
    pub(crate) client_credentials: Option<ClientCredentials>,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Basic {
    pub(crate) user: Option<String>,
    pub(crate) password: Option<String>,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct ClientCredentials {
    pub(crate) token_url: String,
    pub(crate) client_id: String,
    pub(crate) client_secret: String,
}

impl AppConfig {
    pub(crate) fn new() -> Result<Self, ConfigError> {
        Config::builder()
            // default config from file
            .add_source(File::with_name("app.yaml"))
            // override values from environment variables
            .add_source(Environment::default().separator("__"))
            .build()?
            .try_deserialize()
    }
}
