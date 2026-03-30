-- Phase 1.2: User Profile Expansion
-- Expand users table with key profile fields needed for the full user model
-- These fields map to core Prisma schema fields

alter table users
add column if not exists first_name text,
add column if not exists last_name text,
add column if not exists artcc text,
add column if not exists rating text,
add column if not exists division text,
add column if not exists status text default 'ACTIVE',
add column if not exists prefs jsonb default '{}';

-- Create an index on artcc for faster lookups in event/training contexts
create index if not exists idx_users_artcc on users(artcc);
create index if not exists idx_users_status on users(status);

-- Backfill profile fields from display_name if available
-- Extract first and last name from display_name (assumes "First Last" format)
update users
set first_name = split_part(display_name, ' ', 1),
    last_name = case
        when position(' ' in display_name) > 0
        then substring(display_name from position(' ' in display_name) + 1)
        else ''
    end,
    division = 'USA',  -- default division
    status = 'ACTIVE'
where first_name is null;

-- Comment explaining the structure for future migrations
comment on column users.first_name is 'User first name';
comment on column users.last_name is 'User last name';
comment on column users.artcc is 'Associated ARTCC code (e.g., ZBW, ZDC)';
comment on column users.rating is 'VATSIM pilot or controller rating';
comment on column users.division is 'VATSIM division (e.g., USA, EUR)';
comment on column users.status is 'User account status (ACTIVE, INACTIVE, SUSPENDED)';
comment on column users.prefs is 'User preferences stored as JSON (e.g., theme, notifications)';

