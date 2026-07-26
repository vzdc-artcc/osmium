-- The website targets each broadcast at an explicit, admin-chosen set of
-- recipients; osmium's change_broadcasts/change_broadcast_user_state pair
-- only tracked seen/agreed interaction state, with no way to scope who a
-- broadcast is actually addressed to (GET /broadcasts/me returned every
-- broadcast to every user). This table restores that scoping.
create table if not exists web.change_broadcast_recipients (
    broadcast_id text not null references web.change_broadcasts(id) on delete cascade,
    user_id text not null references identity.users(id) on delete cascade,
    created_at timestamptz not null default now(),
    primary key (broadcast_id, user_id)
);

create index if not exists idx_change_broadcast_recipients_user
    on web.change_broadcast_recipients(user_id);
