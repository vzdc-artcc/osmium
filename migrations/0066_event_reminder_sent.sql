-- Event reminder emails (spec 012, Worker B). One "your event is coming up"
-- reminder per event, sent by the event-lifecycle sweep once the event enters the
-- lead window. This column records that the reminder was sent so the periodic
-- sweep does not re-send it on every tick (mirrors training_appointments'
-- warning_email_sent flag).
alter table events.events
    add column if not exists reminder_sent_at timestamptz;
