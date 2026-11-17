-- The callsign the controller logged in with (vNAS `vatsimData.callsign`).
-- It is per VATSIM connection, so it lives on the session; a position's
-- configured `default_callsign` can differ from what was actually used.
alter table stats.controller_sessions add column if not exists callsign text;
