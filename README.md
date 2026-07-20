# 💬 machbarkeit-query

[![MegaLinter](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/mega-linter.yml/badge.svg)](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/mega-linter.yml)
[![build](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/build.yaml/badge.svg)](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/build.yaml)
[![docker](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/release.yaml/badge.svg)](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/release.yaml)
[![codecov](https://codecov.io/gh/diz-unimr/machbarkeit-query/graph/badge.svg?token=Izcyq8RwyX)](https://codecov.io/gh/diz-unimr/machbarkeit-query)


> Feasibility Query Service for the Machbarkeit Web App

This service relays feasibility requests from a broker to a feasibility execution service and sends the result back to
the broker.

The query service communicates with the broker through a websocket connection to retrieve requests and send back
results. The actual (structured) query is send to the execution service with a HTTP request.

The execution service can bei either `cql`  or `flare`.

## CQL

`default`

The default execution service, uses CQL with the help of a translation service in to provide
the query definition (Library) from the request in the _Structured Query_ format.

The following configuration properties are mandatory when using `cql`:

- `feasibility.service`: `cql`
- `feasibility.base_url`: Base url of [diz-unimr/translate](https://github.com/diz-unimr/translate)
- `fhir_server.base_url`: Base url of the FHIR server

## Flare

Executes FHIR search queries via [FLARE](https://github.com/medizininformatik-initiative/flare)
(Feasibility Analysis Request Executor).

The following configuration properties are mandatory when using `flare`:

- `feasibility.service`: `flare`
- `feasibility.base_url`: [FLARE](https://github.com/medizininformatik-initiative/flare) base url

## Configuration properties

Application properties are read from a properties file ([app.yaml](./app.yaml)) with default values.

| Name                                           | Default | Description                                              |
|------------------------------------------------|---------|----------------------------------------------------------|
| `app.log_level`                                | info    | Log level (error,warn,info,debug,trace)                  |
| `feasibility.service`                          | cql     | Feasibility execution service (`cql` or `flare`)         |
| `feasibility.base_url`                         |         | Base url of the query execution service                  |
| `feasibility.auth.basic.user`                  |         | BasicAuth user of the feasibility service (optional)     |
| `feasibility.auth.basic.password`              |         | BasicAuth password of the feasibility service (optional) |
| `fhir_server.base_url`                         |         | Base url of the FHIR server (mandatory fro `cql`)        |
| `fhir_server.auth.basic.user`                  |         | BasicAuth user of the FHIR server (optional)             |
| `fhir_server.auth.basic.password`              |         | BasicAuth password of the FHIR server (optional)         |
| `broker.url`                                   |         | Broker to connect to for requests (ws/wss)               |
| `broker.auth.client_credentials`               |         | OIDC Client Credentials secret                           |
| `broker.auth.client_credentials.token_url`     |         | OIDC Issuer token url                                    |
| `broker.auth.client_credentials.client_id`     |         | OIDC Client id                                           |
| `broker.auth.client_credentials.client_secret` |         | OIDC Client secret                                       |

### Environment variables

Override configuration properties by providing environment variables with their respective property names. Replace `.`
with double underscore (`__`).

## Example deployment

Docker compose:

  ```yaml
query:
  image: ghcr.io/diz-unimr/machbarkeit-query:1.2.0
  environment:
    app.log_level: debug
    feasibility.base_url: http://feasibility/
    fhir_server.base_url: http://fhir-server/fhir
    broker.url: ws://broker/feasibility/ws
    broker.auth.client_credentials.client_id: machbarkeit
    broker.auth.client_credentials.client_secret: ${CLIENT_SECRET}
    broker.auth.client_credentials.token_url: https://idp/auth/realms/Machbarkeit/protocol/openid-connect/token
```

## License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.en.html)
