-- Allow multiple logical channel names to map to the same Discord channel.
--
-- integration.discord_channels previously enforced UNIQUE(channel_id) (migration
-- 0011), which blocked mapping e.g. both `announcements` and `event_announcements`
-- (or `event_position_posting` and another feed) to a single Discord channel.
-- Drop that constraint.
--
-- The per-config unique name constraint (discord_config_id, name) stays, so a
-- logical name is still unique within a config; only channel_id may now be reused
-- across names. Nothing keys off channel_id uniqueness — the bot resolves channels
-- by logical name, and updates/deletes use the row id.
alter table integration.discord_channels
    drop constraint if exists discord_channels_channel_id_key;
