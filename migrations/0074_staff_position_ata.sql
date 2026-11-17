-- ATA (assistant training administrator) was a legacy staff position that
-- 0049 left out of the check, so the legacy import and admins could not store
-- it. Manual-only, like AEC/AWM/AFE: roster sync never sets it.
alter table identity.staff_positions drop constraint staff_positions_position_check;

alter table identity.staff_positions add constraint staff_positions_position_check
    check (position in (
        'ATM', 'DATM', 'TA', 'EC', 'WM', 'FE', 'ATA', 'AEC', 'AWM', 'AFE', 'EP',
        'TMU', 'FC', 'INS', 'MTR'
    ));
