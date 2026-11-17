-- A rubric score records what the session was scored against at the time.
-- Each score now carries its own copy of the criterion and cell it was given
-- for, and no longer has a foreign key to the live lesson rubric, so editing
-- or deleting a criterion or cell leaves past sessions' scores as recorded.
-- criteria_id and cell_id stay as plain text references to the rubric row the
-- score was given against, which may since have been deleted.

alter table training.rubric_scores
    add column criteria text,
    add column criteria_description text,
    add column passing integer,
    add column max_points integer,
    add column cell_points integer,
    add column cell_description text;

update training.rubric_scores s
set criteria = c.criteria,
    criteria_description = c.description,
    passing = c.passing,
    max_points = c.max_points,
    cell_points = cell.points,
    cell_description = cell.description
from training.lesson_rubric_criteria c, training.lesson_rubric_cells cell
where c.id = s.criteria_id
  and cell.id = s.cell_id;

-- The cascading foreign keys guarantee every existing score resolves above;
-- set not null fails the migration rather than leave an unfilled snapshot.
alter table training.rubric_scores
    alter column criteria set not null,
    alter column criteria_description set not null,
    alter column passing set not null,
    alter column max_points set not null,
    alter column cell_points set not null,
    alter column cell_description set not null,
    drop constraint rubric_scores_criteria_id_fkey,
    drop constraint rubric_scores_cell_id_fkey;
