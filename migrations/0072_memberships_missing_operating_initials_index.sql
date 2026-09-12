-- Supports the startup backfill's lookup of active-roster memberships with
-- no operating initials. A partial index scoped to exactly that predicate
-- keeps the query an index scan even once the backlog clears and it returns
-- nothing on every future startup.
create index if not exists idx_org_memberships_missing_operating_initials
    on org.memberships (controller_status)
    where operating_initials is null;
