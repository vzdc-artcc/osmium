# Architecture Overview

Osmium is the shared backend and API system of record for current vZDC apps and bots.

## Core Shape

- One Axum application
- One primary Postgres database
- Multiple database schemas by domain
- Shared auth and ACL
- Shared web/publication content model
- Shared files and CDN behavior
- Domain-specific handlers and repos

## Why This Shape

The platform needs shared user, permission, training, event, publication, and file data. Splitting those into app-owned databases would recreate sync and ownership problems immediately.

## High-Level Flow

- request enters Axum router
- auth middleware resolves user session or service account
- handler validates permissions
- repo/query layer performs DB access
- handler returns JSON or file/HTML response

## Major Consumers

- website and future web clients
- internal admin surfaces
- bots
- integration or sync jobs

The website-facing side now includes a first-class publications/downloads catalog in `web.publications` and `web.publication_categories`, with linked file blobs still owned by `media.file_assets`.
