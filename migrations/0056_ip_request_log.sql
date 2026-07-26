-- spec 011 — durable per-request IP tracking.
--
-- Records metadata for EVERY request (reads, unauthenticated traffic, failed and
-- throttled requests) — method, matched route template, status, resolved actor,
-- timestamp. Deliberately excludes bodies, query strings and headers: this is a
-- narrow, high-volume-tolerant table, distinct from access.audit_logs (which keeps
-- before/after JSON for specific write actions and is unaffected by this spec).
create table access.ip_request_log (
    id bigserial primary key,
    ip_address inet not null,
    method text not null,
    -- Route template (e.g. "/api/v1/users/{cid}"), not the raw path, so aggregation
    -- by endpoint doesn't explode on path parameters.
    matched_path text not null,
    status_code smallint not null,
    -- 'user' | 'service_account' | null, mirrors logging.rs::auth_mode.
    actor_type text,
    actor_id text references access.actors(id) on delete set null,
    created_at timestamptz not null default now()
);

-- Per-IP forensic lookups + the retention sweep both scan (ip, created_at).
create index ip_request_log_ip_created_idx on access.ip_request_log (ip_address, created_at);
-- The new per-user admin endpoint filters by the resolved actor.
create index ip_request_log_actor_created_idx on access.ip_request_log (actor_id, created_at);
