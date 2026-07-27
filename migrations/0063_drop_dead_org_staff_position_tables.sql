-- Drop the legacy org.staff_positions / org.user_staff_positions tables.
--
-- These were superseded by identity.staff_positions (migration 0049) and are dead:
-- nothing in the osmium app reads them. The live staff-position system is the
-- STAFF_POSITIONS constant + identity.staff_positions, populated by VATUSA roster
-- sync (source='auto', manual overrides win) unless a user is excluded from sync.
--
-- Child (user_staff_positions) first, then the parent.
drop table if exists org.user_staff_positions;
drop table if exists org.staff_positions;
