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
    pub(crate) fn execute(
        &self,
        request: FeasibilityRequest,
    ) -> impl Future<Output = anyhow::Result<FeasibilityRequest>> + Send {
        match self {
            TargetClient::Cql(cql) => cql.execute(request).boxed(),
            TargetClient::Flare(flare) => flare.execute(request).boxed(),
        }
    }
}

struct HttpEndpoint {
    base_url: String,
    auth: Option<Auth>,
}
