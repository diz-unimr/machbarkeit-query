# 💬 machbarkeit-query

[![MegaLinter](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/mega-linter.yml/badge.svg)](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/mega-linter.yml)
[![build](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/build.yaml/badge.svg)](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/build.yaml)
[![docker](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/release.yaml/badge.svg)](https://github.com/diz-unimr/machbarkeit-query/actions/workflows/release.yaml)
[![codecov](https://codecov.io/gh/diz-unimr/machbarkeit-query/graph/badge.svg?token=Izcyq8RwyX)](https://codecov.io/gh/diz-unimr/machbarkeit-query)


> Feasibility Query Service for the Machbarkeit Web App

This service relays feasibility requests from a broker to a feasibility execution service and sends the result back to
the broker.

Currently, only [FLARE](https://github.com/medizininformatik-initiative/flare) (Feasibility Analysis Request Executor)
with
the [Structured Query](https://github.com/num-codex/codex-structured-query/blob/main/structured-query/documentation/2021_01_29StructeredQueriesDocumentation(Draft).md)
format is supported.

The query service communicates with the broker through a websocket connection to retrieve requests and send back
results. The actual (structured) query is send to the execution service with a HTTP request.

## Configuration properties

Application properties are read from a properties file ([app.yaml](./app.yaml)) with default values.

| Name                             | Default | Description                                    |
|----------------------------------|---------|------------------------------------------------|
| `app.log_level`                  | info    | Log level (error,warn,info,debug,trace)        |
| `feasibility.base_url`           |         | Base url of the (FLARE) query execute endpoint |
| `broker.url`                     |         | Broker to connect to for requests (wss)        |
| `broker.auth.client_credentials` |         | OIDC Client Credentials secret                 |
| `broker.auth.token_url`          |         | OIDC Issuer token url                          |
| `broker.auth.client_id`          |         | OIDC Client id                                 |
| `broker.auth.client_secret`      |         | OIDC Client secret                             |

### Environment variables

Override configuration properties by providing environment variables with their respective property names. Replace `.`
with double underscore (`__`).

## Example deployment

Docker compose:

```yaml
query:
  image: ghcr.io/diz-unimr/machbarkeit-query:1.1.2
  environment:
    APP__LOG_LEVEL: debug
    FEASIBILITY__BASE_URL: http://flare/query/execute
    BROKER__URL: ws://broker/feasibility/ws
    BROKER__AUTH__CLIENT_CREDENTIALS__CLIENT_ID: machbarkeit
    BROKER__AUTH__CLIENT_CREDENTIALS__CLIENT_SECRET: ${CLIENT_SECRET}
    BROKER__AUTH__CLIENT_CREDENTIALS__TOKEN_URL: https://idp/auth/realms/Machbarkeit/protocol/openid-connect/token
```

## License

[AGPL-3.0](https://www.gnu.org/licenses/agpl-3.0.en.html)
