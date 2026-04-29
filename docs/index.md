# Osmium Docs

Osmium is the shared backend and API platform for vZDC apps, bots, files, training, events, feedback, and statistics.

## What Osmium Owns

- Shared identity and session handling
- Access control and effective permissions
- ARTCC roster and membership state
- Training workflows and requests
- Event management and staffing
- Feedback workflows
- File metadata, storage policy, and CDN delivery
- Statistics and sync jobs
- Internal integration and service-account support

## Current Domain Map

- `identity`: users, sessions, linked identities
- `access`: roles, permissions, assignments, service accounts
- `org`: roster and controller-state data
- `training`: assignments, requests, release workflows
- `events`: event lifecycle and positions
- `feedback`: controller feedback records
- `media`: assets, audit, file access
- `stats`: controller hours and sync state
- `integration`: external sync and machine clients
- `web`: website-oriented shared content

## Start Here

- Local setup: [/docs/getting-started/local-development](/docs/getting-started/local-development)
- Configuration: [/docs/getting-started/configuration](/docs/getting-started/configuration)
- Migrations: [/docs/getting-started/migrations](/docs/getting-started/migrations)
- Testing: [/docs/getting-started/testing](/docs/getting-started/testing)

## Architecture

- Overview: [/docs/architecture/overview](/docs/architecture/overview)
- Request flow: [/docs/architecture/request-flow](/docs/architecture/request-flow)
- Auth and access: [/docs/architecture/auth-and-access](/docs/architecture/auth-and-access)
- Data domains: [/docs/architecture/data-domains](/docs/architecture/data-domains)
- Files and CDN: [/docs/architecture/files-and-cdn](/docs/architecture/files-and-cdn)

## API Docs

- Narrative API overview: [/docs/api/overview](/docs/api/overview)
- Publications API: [/docs/api/publications](/docs/api/publications)
- Interactive OpenAPI reference: [/docs/api/v1](/docs/api/v1)

## Operations and Maintenance

- Jobs and sync: [/docs/operations/jobs-and-sync](/docs/operations/jobs-and-sync)
- Service accounts: [/docs/operations/service-accounts](/docs/operations/service-accounts)
- Troubleshooting: [/docs/operations/troubleshooting](/docs/operations/troubleshooting)

## Contributor Guidance

- Code organization: [/docs/contributors/code-organization](/docs/contributors/code-organization)
- Adding routes: [/docs/contributors/adding-routes](/docs/contributors/adding-routes)
- Documenting endpoints: [/docs/contributors/documenting-endpoints](/docs/contributors/documenting-endpoints)
