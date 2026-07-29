-- Runtime on/off toggles for segments of the Discord bot (staffup, commands,
-- announcement posts, event postings, scheduled events, etc.), managed from the
-- Website Management "Bot Features" section and read by the bot on its config
-- sync. The canonical feature list lives in code; this table only stores
-- overrides, so a feature with no row defaults to enabled.
create table if not exists integration.bot_feature_flags (
    feature text primary key,
    enabled boolean not null default true,
    updated_at timestamptz not null default now()
);
