-- Phase 2: Event Domain Parity
-- Extends events and event_positions to support full lifecycle (signup, assignment, publishing)

-- Expand events table
ALTER TABLE events ADD COLUMN IF NOT EXISTS type varchar(50) default 'STANDARD';
ALTER TABLE events ADD COLUMN IF NOT EXISTS host varchar(255);
ALTER TABLE events ADD COLUMN IF NOT EXISTS description text;
ALTER TABLE events ADD COLUMN IF NOT EXISTS status varchar(50) default 'SCHEDULED';
ALTER TABLE events ADD COLUMN IF NOT EXISTS published boolean default false;
ALTER TABLE events ADD COLUMN IF NOT EXISTS updated_at timestamptz default now();

-- Add indexes for events
CREATE INDEX IF NOT EXISTS idx_events_status ON events(status);
CREATE INDEX IF NOT EXISTS idx_events_published ON events(published);
CREATE INDEX IF NOT EXISTS idx_events_host ON events(host);

-- Expand event_positions table
ALTER TABLE event_positions ADD COLUMN IF NOT EXISTS requested_slot int;
ALTER TABLE event_positions ADD COLUMN IF NOT EXISTS assigned_slot int;
ALTER TABLE event_positions ADD COLUMN IF NOT EXISTS published boolean default false;
ALTER TABLE event_positions ADD COLUMN IF NOT EXISTS status varchar(50) default 'OPEN';
ALTER TABLE event_positions ADD COLUMN IF NOT EXISTS updated_at timestamptz default now();

-- Add uniqueness constraints (if not already present)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'unique_event_callsign'
  ) THEN
    ALTER TABLE event_positions
      ADD CONSTRAINT unique_event_callsign UNIQUE (event_id, callsign);
  END IF;
END $$;

CREATE UNIQUE INDEX IF NOT EXISTS idx_event_positions_event_assigned_slot_unique
ON event_positions(event_id, assigned_slot)
WHERE assigned_slot IS NOT NULL;

-- Add indexes for event_positions
CREATE INDEX IF NOT EXISTS idx_event_positions_status ON event_positions(status);
CREATE INDEX IF NOT EXISTS idx_event_positions_published ON event_positions(published);
CREATE INDEX IF NOT EXISTS idx_event_positions_assigned_slot ON event_positions(assigned_slot);

-- Create event_tmis table
CREATE TABLE IF NOT EXISTS event_tmis (
  id text primary key,
  event_id text not null references events(id) on delete cascade,
  tmi_type varchar(50) not null,
  start_time timestamptz not null,
  notes text,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

CREATE INDEX IF NOT EXISTS idx_event_tmis_event_id ON event_tmis(event_id);
CREATE INDEX IF NOT EXISTS idx_event_tmis_tmi_type ON event_tmis(tmi_type);

-- Create ops_plan_files table
CREATE TABLE IF NOT EXISTS ops_plan_files (
  id text primary key,
  event_id text not null references events(id) on delete cascade,
  filename text not null,
  url text,
  file_type varchar(50),
  uploaded_by text references users(id),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

CREATE INDEX IF NOT EXISTS idx_ops_plan_files_event_id ON ops_plan_files(event_id);
CREATE INDEX IF NOT EXISTS idx_ops_plan_files_file_type ON ops_plan_files(file_type);

