use crate::client::HttpEndpoint;
use crate::config::{Feasibility, FhirServer};
use crate::model::{FeasibilityRequest, QueryState};
use anyhow::{anyhow, Error};
use base64::{engine::general_purpose, Engine as _};
use http::header::CONTENT_TYPE;
use http::{Method, StatusCode};
use log::{debug, info};
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
    resource_type: String,
    url: String,
    status: String,
    #[serde(rename = "type")]
    library_type: CodeableConcept,
    content: Vec<ContentData>,
}

impl From<Bytes> for Library {
    fn from(value: Bytes) -> Self {
        Library {
            resource_type: "Library".to_string(),
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
#[serde(rename_all = "camelCase")]
struct Measure {
    resource_type: String,
    url: String,
    status: String,
    subject_codeable_concept: CodeableConcept,
    library: Vec<String>,
    scoring: CodeableConcept,
    group: Vec<MeasurePopulationGroup>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum MeasureResponse {
    MeasureReport(MeasureReport),
    OperationOutcome(OperationOutcome),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OperationOutcome {
    issue: Vec<OutcomeIssue>,
}

#[derive(Deserialize)]
struct OutcomeIssue {
    diagnostics: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MeasureReport {
    status: String,
    group: Vec<MeasureReportPopulationGroup>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Bundle {
    resource_type: String,
    entry: Vec<BundleEntry>,
    #[serde(rename = "type")]
    r_type: String,
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
#[serde(untagged)]
enum Resource {
    Library(Library),
    Measure(Measure),
}

impl CqlClient {
    pub(crate) fn new(feasibility: &Feasibility, fhir_server: &FhirServer) -> Result<Self, Error> {
        Ok(CqlClient {
            translate: HttpEndpoint {
                base_url: feasibility.base_url.clone(),
                auth: feasibility.auth.clone(),
            },
            fhir_server: HttpEndpoint {
                base_url: fhir_server.base_url.clone(),
                auth: fhir_server.auth.clone(),
            },
            client: Client::builder().build()?,
        })
    }

    async fn translate(&self, query: &Value) -> Result<Bytes, Error> {
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
        let measure_id = format!("urn:uuid:{}", Uuid::new_v4());
        let measure = Measure {
            resource_type: "Measure".to_string(),
            url: measure_id.clone(),
            status: "active".to_string(),
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
            resource_type: "Bundle".to_string(),
            r_type: "transaction".to_string(),
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
        let response = self
            .build_request(&self.fhir_server, "", Method::POST)
            .json(&payload)
            .header(CONTENT_TYPE, "application/fhir+json")
            .send()
            .await?;

        let status = response.status();
        let resp_text = response.text().await?;
        debug!("Response from FHIR server: {}", resp_text);
        if !status.is_success() {
            return Ok(request.result(QueryState::Completed, status.as_u16(), resp_text));
        }

        info!(
            "Evaluate measure({}) for request({}) on {}",
            measure_id, request.id, self.fhir_server.base_url
        );
        let response = self
            .build_request(
                &self.fhir_server,
                format!(
                    "/Measure/$evaluate-measure?measure={}&periodStart=2000&periodEnd=2030",
                    measure_id
                )
                .as_str(),
                Method::GET,
            )
            .send()
            .await?;

        let resp_text = response.text().await?;
        debug!("Measure response: {}", resp_text);
        let response: MeasureResponse = serde_json::from_str(resp_text.as_str())?;

        // parse response
        let report = match response {
            MeasureResponse::MeasureReport(report) => Ok(report),
            MeasureResponse::OperationOutcome(outcome) => Err(anyhow!(
                "Error from $evaluate-measure: {}",
                outcome
                    .issue
                    .iter()
                    .map(|i| i.diagnostics.as_str())
                    .collect::<Vec<&str>>()
                    .join(", ")
            )),
        }?;

        let request = set_result(request, &report)?;

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

fn set_result(
    request: FeasibilityRequest,
    report: &MeasureReport,
) -> anyhow::Result<FeasibilityRequest> {
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
    Ok(request.result(QueryState::Completed, status.as_u16(), result))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::QueryState::{Completed, Pending};
    use chrono::Utc;

    #[test]
    fn test_set_result() {
        let request = FeasibilityRequest {
            id: Uuid::new_v4(),
            status: Pending,
            query: Value::Null,
            date: Utc::now(),
            result_duration: None,
            result_code: None,
            result_body: None,
        };
        let expected = FeasibilityRequest {
            id: request.id,
            status: Completed,
            query: Value::Null,
            date: request.date,
            result_duration: None,
            result_code: Some(200),
            result_body: Some(42.to_string()),
        };

        let report = MeasureReport {
            status: "complete".to_string(),
            group: vec![MeasureReportPopulationGroup {
                population: vec![PopulationGroupCount {
                    code: CodeableConcept {
                        coding: vec![Coding {
                            system: "http://terminology.hl7.org/CodeSystem/measure-population"
                                .to_string(),
                            code: "initial-population".to_string(),
                        }],
                    },
                    count: 42,
                }],
            }],
        };

        let actual = set_result(request, &report).unwrap();

        assert_eq!(actual, expected);
    }
}
