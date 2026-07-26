-- TMIs are being simplified to just a type + free-text description (no scheduled
-- start time). Make the legacy start_time nullable so new TMIs can omit it; the
-- column is kept (not dropped) so any existing rows retain their value.
alter table events.event_tmis
    alter column start_time drop not null;
