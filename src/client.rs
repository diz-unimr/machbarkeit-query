pub mod cql;
pub mod flare;

use crate::client::cql::CqlClient;
use crate::client::flare::FlareClient;
use crate::config::Auth;
use crate::model::FeasibilityRequest;
use futures_util::FutureExt;

#[allow(clippy::large_enum_variant)]
pub(crate) enum TargetClient {
    Cql(CqlClient),
    Flare(FlareClient),
}

impl TargetClient {
    pub(crate) async fn execute(
        &self,
        request: FeasibilityRequest,
    ) -> anyhow::Result<FeasibilityRequest> {
        match self {
            TargetClient::Cql(cql) => cql.execute(request).boxed().await,
            TargetClient::Flare(flare) => flare.execute(request).boxed().await,
        }
    }
}

struct HttpEndpoint {
    base_url: String,
    auth: Option<Auth>,
}
