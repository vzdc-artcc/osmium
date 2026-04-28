# Integrations

Osmium is designed to be the shared backend for current and future apps and bots.

## Current Integration Themes

- VATSIM OAuth and user identity
- service-account auth for internal clients
- stats sync jobs
- file/CDN consumption by other apps

## Service Accounts

Service accounts are the preferred model for bots and internal machine clients. They should not impersonate human rows for normal integration behavior.

## Future Direction

Expected integration expansion:

- richer bot write paths
- explicit webhook ingestion flows
- external sync mapping tables
- app-specific credentials scoped by role and permission
