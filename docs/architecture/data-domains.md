# Data Domains

Osmium uses domain-separated schemas inside one database.

## identity

Owns users, profiles, flags, linked identities, sessions, and verification state.

Identity ownership includes:

- `identity.users.preferred_name`
- `identity.user_profiles.bio`
- `identity.user_profiles.timezone`
- `identity.user_profiles.new_event_notifications`
- `identity.user_identities` rows for linked TeamSpeak UIDs

## access

Owns roles, permissions, direct overrides, service accounts, credentials, and audit.

## org

Owns ARTCC membership, controller status, staff positions, certifications, and related roster state.

Org ownership includes `org.memberships.operating_initials`, even though those initials are bootstrapped during login from the auth flow.

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

Media remains the source of truth for file blobs and CDN delivery even when another domain record, such as a web publication, owns the metadata relationship.

## stats

Owns controller time history, sync timestamps, and ARTCC-level rollups.

## integration

Owns provider-specific sync and machine-to-machine state.

## web

Owns website-oriented shared content that belongs in the platform layer.

This includes publication categories and publication metadata for the public downloads area, while linked file assets continue to live in `media`.
