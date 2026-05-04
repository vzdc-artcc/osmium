do $$
begin
    if exists (
        select 1
        from training.ots_recommendations
        group by student_id
        having count(*) > 1
    ) then
        raise exception using
            message = 'Cannot add unique index on training.ots_recommendations(student_id): duplicate student_id rows exist.',
            hint = 'Clean up duplicate OTS recommendations before rerunning migration 0025_ots_recommendation_uniqueness.sql.';
    end if;
end $$;

create unique index if not exists idx_ots_recommendations_student_id_unique
on training.ots_recommendations (student_id);
