# Data Domains

Osmium uses domain-separated schemas inside one database.

## identity

Owns users, profiles, flags, linked identities, sessions, and verification state.

## access

Owns roles, permissions, direct overrides, service accounts, credentials, and audit.

## org

Owns ARTCC membership, controller status, staff positions, certifications, and related roster state.

## training

Owns assignments, assignment requests, release requests, lesson/progression structures, training appointments, OTS recommendations, and training-session workflows.

Training-session workflow ownership includes:

- `training_sessions`
- `training_tickets`
- `rubric_scores`
- `session_performance_indicators`
- lesson-linked roster-change rules that drive org and dossier side effects

## events

Owns event records, event positions, staffing assignments, and publish flows.

## feedback

Owns controller feedback and moderation/release decisions.

## media

Owns file metadata, access policy, audit logs, and storage references.

## stats

Owns controller time history, sync timestamps, and ARTCC-level rollups.

## integration

Owns provider-specific sync and machine-to-machine state.

## web

Owns website-oriented shared content that belongs in the platform layer.
