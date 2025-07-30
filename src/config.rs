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
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Server {
    pub(crate) base_url: String,
    pub(crate) auth: Option<Auth>,
    pub(crate) retry: Retry,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Auth {
    pub(crate) basic: Option<Basic>,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Basic {
    pub(crate) user: Option<String>,
    pub(crate) password: Option<String>,
}

#[derive(Default, Debug, Deserialize, Clone)]
pub(crate) struct Retry {
    pub(crate) count: u32,
    pub(crate) timeout: u64,
    pub(crate) wait: u64,
    pub(crate) max_wait: u64,
}

impl AppConfig {
    pub(crate) fn new() -> Result<Self, ConfigError> {
        Config::builder()
            // default config from file
            .add_source(File::with_name("app.yaml"))
            // override values from environment variables
            .add_source(Environment::default().separator("_"))
            .build()?
            .try_deserialize()
    }
}
