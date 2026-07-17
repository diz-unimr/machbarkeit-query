use crate::client::HttpEndpoint;
use crate::config::{Feasibility, FhirServer};
use crate::model::{FeasibilityRequest, QueryState};
use anyhow::{anyhow, Error};
use base64::{engine::general_purpose, Engine as _};
use http::header::CONTENT_TYPE;
use http::{Method, StatusCode};
use log::info;
use reqwest::{Client, RequestBuilder};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;
use tokio_tungstenite::tungstenite::Bytes;
use uuid::Uuid;

pub(crate) struct CqlClient {
    translate: HttpEndpoint,
    fhir_server: HttpEndpoint,
    client: Client,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ContentData {
    content_type: String,
    data: String,
}

#[derive(Serialize, Deserialize)]
struct CodeableConcept {
    coding: Vec<Coding>,
}

#[derive(Serialize, Deserialize)]
struct Coding {
    system: String,
    code: String,
}

#[derive(Serialize)]
struct Criteria {
    language: String,
    expression: String,
}

#[derive(Serialize)]
struct PopulationGroupCriteria {
    code: CodeableConcept,
    criteria: Criteria,
}

#[derive(Deserialize)]
struct PopulationGroupCount {
    code: CodeableConcept,
    count: u8,
}

#[derive(Serialize)]
struct MeasurePopulationGroup {
    population: Vec<PopulationGroupCriteria>,
}

#[derive(Deserialize)]
struct MeasureReportPopulationGroup {
    population: Vec<PopulationGroupCount>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Library {
    resource_type: &'static str,
    url: String,
    status: String,
    #[serde(rename = "type")]
    library_type: CodeableConcept,
    content: Vec<ContentData>,
}

impl From<Bytes> for Library {
    fn from(value: Bytes) -> Self {
        Library {
            resource_type: "Library",
            status: "active".to_string(),
            library_type: CodeableConcept {
                coding: vec![{
                    Coding {
                        system: "http://terminology.hl7.org/CodeSystem/library-type".to_string(),
                        code: "logic-library".to_string(),
                    }
                }],
            },
            url: format!("urn:uuid:{}", Uuid::new_v4()),
            content: vec![ContentData {
                content_type: "text/cql".to_string(),
                data: general_purpose::STANDARD.encode(value),
            }],
        }
    }
}

#[derive(Serialize)]
struct Measure {
    url: String,
    status: String,
    subject_codeable_concept: CodeableConcept,
    library: Vec<String>,
    scoring: CodeableConcept,
    group: Vec<MeasurePopulationGroup>,
}

#[derive(Serialize, Deserialize)]
struct Quantity {
    value: String,
    system: String,
    code: String,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EvalDuration {
    url: String,
    value_quantity: Quantity,
}

#[derive(Deserialize)]
struct MeasureReport {
    status: String,
    extension: Vec<EvalDuration>,
    group: Vec<MeasureReportPopulationGroup>,
}

#[derive(Serialize)]
struct Bundle {
    entry: Vec<BundleEntry>,
}

#[derive(Serialize)]
struct BundleEntry {
    resource: Resource,
    request: BundleEntryRequest,
}

#[derive(Serialize)]
struct BundleEntryRequest {
    method: String,
    url: String,
}

#[derive(Serialize)]
enum Resource {
    Library(Library),
    Measure(Measure),
}

impl CqlClient {
    pub(crate) fn new(feasibility: &Feasibility, fhir_server: &FhirServer) -> Result<Self, Error> {
        Ok(CqlClient {
            // todo
            translate: HttpEndpoint {
                base_url: feasibility.base_url.clone(),
                auth: feasibility.auth.clone(),
            },
            // todo
            fhir_server: HttpEndpoint {
                base_url: fhir_server.base_url.clone(),
                auth: fhir_server.auth.clone(),
            },
            client: Client::builder().build()?,
        })
    }

    async fn translate(&self, query: &Value) -> Result<Bytes, Error> {
        // todo
        let builder = self.build_request(&self.translate, "/translate", Method::POST);

        let response = builder
            .header("content-type", "application/sq+json")
            .json(query)
            .send()
            .await?;

        response.bytes().await.map_err(Error::from)
    }

    pub(crate) async fn execute(
        &self,
        request: FeasibilityRequest,
    ) -> Result<FeasibilityRequest, Error> {
        info!(
            "Translate request id={} to {}",
            request.id, self.translate.base_url
        );

        let library: Library = self.translate(&request.query).await?.into();
        let measure = Measure {
            url: format!("urn:uuid:{}", Uuid::new_v4()),
            status: "".to_string(),
            subject_codeable_concept: CodeableConcept {
                coding: vec![Coding {
                    system: "http://hl7.org/fhir/resource-types".to_string(),
                    code: "Patient".to_string(),
                }],
            },
            library: vec![library.url.clone()],
            scoring: CodeableConcept {
                coding: vec![Coding {
                    system: "http://terminology.hl7.org/CodeSystem/measure-scoring".to_string(),
                    code: "cohort".to_string(),
                }],
            },
            group: vec![MeasurePopulationGroup {
                population: vec![PopulationGroupCriteria {
                    code: CodeableConcept {
                        coding: vec![Coding {
                            system: "http://terminology.hl7.org/CodeSystem/measure-population"
                                .to_string(),
                            code: "initial-population".to_string(),
                        }],
                    },
                    criteria: Criteria {
                        language: "text/cql-identifier".to_string(),
                        expression: "InInitialPopulation".to_string(),
                    },
                }],
            }],
        };
        let payload = Bundle {
            entry: vec![
                BundleEntry {
                    resource: Resource::Library(library),
                    request: BundleEntryRequest {
                        method: Method::POST.to_string(),
                        url: "Library".to_string(),
                    },
                },
                BundleEntry {
                    resource: Resource::Measure(measure),
                    request: BundleEntryRequest {
                        method: Method::POST.to_string(),
                        url: "Measure".to_string(),
                    },
                },
            ],
        };
        info!(
            "Create Library + Measure resources id={} on={}",
            request.id, self.fhir_server.base_url
        );
        self.build_request(&self.fhir_server, "", Method::POST)
            .json(&payload)
            .header(CONTENT_TYPE, "application/fhir+json")
            .send()
            .await?;

        info!(
            "Evaluate Measure request id={} on {}",
            request.id, self.fhir_server.base_url
        );
        let response = self
            .build_request(
                &self.fhir_server,
                format!("/Measure/$evaluate-measure?measure=urn:uuid:{}", request.id).as_str(),
                Method::GET,
            )
            .send()
            .await?;
        let report: MeasureReport = serde_json::from_str(response.text().await?.as_str())?;
        // TODO parse eval-duration and log

        let status = match report.status.as_str() {
            "complete" => StatusCode::OK,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        let result: String = report
            .group
            .iter()
            .flat_map(|g| g.population.iter())
            .find_map(|g| {
                if g.code.coding.iter().any(|c| {
                    c.system == "http://terminology.hl7.org/CodeSystem/measure-population"
                        && c.code == "initial-population"
                }) {
                    Some(g.count.to_string())
                } else {
                    None
                }
            })
            .ok_or(anyhow!("Failed to parse result from MeasureReport"))?;

        let request = request.result(QueryState::Completed, status.as_u16(), result);
        Ok(request)
    }

    fn build_request(&self, endpoint: &HttpEndpoint, path: &str, method: Method) -> RequestBuilder {
        let mut builder = self
            .client
            .request(method, format!("{}{}", &endpoint.base_url, path));
        if let Some(basic) = endpoint.auth.as_ref().and_then(|a| a.basic.as_ref()) {
            builder = builder.basic_auth(&basic.user, Some(&basic.password));
        }

        builder
    }
}
