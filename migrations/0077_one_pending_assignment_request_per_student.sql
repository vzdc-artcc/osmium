-- A student could hold only one training assignment request ever: student_id
-- was unique and deciding a request keeps its row, so after one approval or
-- denial every later request failed. Allow one *pending* request per student
-- and keep decided ones as history.
alter table training.training_assignment_requests
    drop constraint if exists training_assignment_requests_student_id_key;

create unique index if not exists training_assignment_requests_one_pending_per_student
    on training.training_assignment_requests (student_id)
    where status = 'PENDING';

create index if not exists training_assignment_requests_student_id_idx
    on training.training_assignment_requests (student_id);
