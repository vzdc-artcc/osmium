-- Statistics only record the live network; sweatbox (training) time is not
-- controlling time.
delete from stats.controller_events where environment <> 'live';
delete from stats.controller_monthly_rollups where environment <> 'live';
delete from stats.controller_activations where environment <> 'live';
delete from stats.controller_sessions where environment <> 'live';
delete from stats.controller_feed_state where environment <> 'live';

-- Tower-cab positions are stored by role (Delivery/Ground/Tower), resolved
-- from the default callsign's suffix, the same rule as `cab_position_type`.
-- Rows still typed `Atct` are reclassified here.
create temporary table tmp_atct_reclassified on commit drop as
select
    id,
    environment,
    cid,
    is_primary,
    started_at,
    ended_at,
    case upper(substring(default_callsign from '_([^_]+)$'))
        when 'DEL' then 'Delivery'
        when 'GND' then 'Ground'
        when 'TWR' then 'Tower'
    end as position_type
from stats.controller_activations
where position_type = 'Atct';

delete from tmp_atct_reclassified where position_type is null;

update stats.controller_activations a
set position_type = r.position_type
from tmp_atct_reclassified r
where a.id = r.id;

-- An `Atct` activation contributed nothing to the position buckets when it
-- closed, so each closed primary one is credited now: split at UTC month
-- boundaries with whole seconds per segment, as `monthly_segments` and
-- `add_monthly_rollup` do. Month arithmetic runs on UTC wall-clock
-- timestamps so the session TimeZone cannot shift a boundary. Online seconds
-- were already counted.
insert into stats.controller_monthly_rollups (
    environment, cid, year, month,
    online_seconds, delivery_seconds, ground_seconds, tower_seconds, tracon_seconds, center_seconds
)
select
    environment,
    cid,
    year,
    month,
    0,
    coalesce(sum(seconds) filter (where position_type = 'Delivery'), 0),
    coalesce(sum(seconds) filter (where position_type = 'Ground'), 0),
    coalesce(sum(seconds) filter (where position_type = 'Tower'), 0),
    0,
    0
from (
    select
        r.environment,
        r.cid,
        r.position_type,
        extract(year from months.month_start)::int as year,
        extract(month from months.month_start)::int - 1 as month,
        floor(extract(epoch from (
            least(r.ended_at, (months.month_start + interval '1 month') at time zone 'UTC')
            - greatest(r.started_at, months.month_start at time zone 'UTC')
        )))::bigint as seconds
    from tmp_atct_reclassified r
    cross join lateral generate_series(
        date_trunc('month', r.started_at at time zone 'UTC'),
        date_trunc('month', r.ended_at at time zone 'UTC'),
        interval '1 month'
    ) as months(month_start)
    where r.is_primary and r.ended_at is not null and r.ended_at > r.started_at
) split
where seconds > 0
group by environment, cid, year, month
on conflict (environment, cid, year, month) do update set
    delivery_seconds = stats.controller_monthly_rollups.delivery_seconds
        + excluded.delivery_seconds,
    ground_seconds = stats.controller_monthly_rollups.ground_seconds
        + excluded.ground_seconds,
    tower_seconds = stats.controller_monthly_rollups.tower_seconds
        + excluded.tower_seconds,
    updated_at = now();
