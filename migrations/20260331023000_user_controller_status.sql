-- Add explicit controller_status to align with Prisma ControllerStatus enum.

alter table users
add column if not exists controller_status text not null default 'NONE';

DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1
    FROM pg_constraint
    WHERE conname = 'users_controller_status_check'
  ) THEN
    alter table users
      add constraint users_controller_status_check
      check (controller_status in ('HOME', 'VISITOR', 'NONE'));
  END IF;
END $$;

create index if not exists idx_users_controller_status on users(controller_status);

update users
set controller_status = 'NONE'
where controller_status is null or controller_status not in ('HOME', 'VISITOR', 'NONE');

