-- Provision an audit actor for every user (audit-attribution fix).
--
-- Regular users never received an `access.actors` row (only service accounts did,
-- via api_keys), so `resolve_audit_actor` -> `lookup_user_actor_id` always returned
-- null for a human. The effect: every user-attributed audit entry landed with a null
-- actor (silently rendering as "system"), and the per-user IP history (spec 011) could
-- never resolve a user's requests. This also undercut the impersonation feature's
-- accountability guarantee (spec 012), which depends on the impersonator's actor.
--
-- Fix: enforce one 'user' actor per user with a partial unique index, backfill every
-- existing user, and (in code) provision the row on each login bootstrap.

create unique index if not exists access_actors_unique_user
    on access.actors (user_id)
    where actor_type = 'user';

insert into access.actors (actor_type, user_id, display_name)
select
    'user',
    u.id,
    coalesce(nullif(u.display_name, ''), nullif(u.full_name, ''), 'CID ' || u.cid::text)
from identity.users u
where not exists (
    select 1 from access.actors a where a.user_id = u.id and a.actor_type = 'user'
)
on conflict do nothing;
