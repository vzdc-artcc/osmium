-- Granular self-service email preferences: add suppression categories for the
-- email template categories that previously had no matching opt-out category
-- (training, feedback, org), so users can opt out of them via the new
-- `GET/PUT /api/v1/me/email-preferences` endpoint. `resolve_recipients` already
-- gates non-transactional mail by matching `template.category` against
-- `email.suppressions`, so once these category rows exist those emails become
-- opt-out-able with no template changes.
--
-- All three are non-transactional (opt-out, default subscribed). Transactional
-- mail stays locked/always-on.
insert into email.suppression_categories (id, name, description, is_transactional)
values
    ('training', 'Training', 'Training appointments, sessions, and progression updates', false),
    ('feedback', 'Feedback', 'Notifications when feedback about you is released', false),
    ('org', 'Roster & Membership', 'Roster, membership, and visitor-application notices', false)
on conflict (id) do update
set name = excluded.name,
    description = excluded.description,
    is_transactional = excluded.is_transactional;

-- Note: the legacy `identity.user_profiles.new_event_notifications` opt-in flag is
-- retired by this change set (the unified opt-out category system supersedes it).
-- The column is intentionally left in place (unused/deprecated) rather than dropped,
-- to avoid a destructive migration.
