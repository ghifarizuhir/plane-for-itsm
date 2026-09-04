#!/bin/bash
# Plan 3.2: keep normalized schema vs jsonb alternative.
set -e
PSQL="docker exec plane-db psql -U plane -d plane -c"
P=$(docker exec plane-db psql -U plane -d plane -t -A -c "SELECT id FROM projects LIMIT 1;")
echo "--- Row counts ---"
$PSQL "SELECT (SELECT count(*) FROM issues) AS issues, (SELECT count(*) FROM projects) AS projects;"
echo "--- Keep schema: filter by project_id + archived_at, order name ---"
$PSQL "EXPLAIN (ANALYZE, TIMING OFF) SELECT id, name FROM issues WHERE project_id='$P' AND archived_at IS NULL ORDER BY name LIMIT 100;" | tail -n 5
echo "--- Keep schema: priority as plain column ---"
$PSQL "EXPLAIN (ANALYZE, TIMING OFF) SELECT id FROM issues WHERE project_id='$P' AND priority='high' LIMIT 100;" | tail -n 5
echo "--- Jsonb alternative (hypothetical properties->>'priority'): no GIN index exists, seq scan forced ---"
$PSQL "EXPLAIN SELECT id FROM issues WHERE project_id='$P' AND (properties::jsonb ->> 'priority')='high' LIMIT 100;" 2>&1 | tail -n 4 || true
echo "--- Index + table size (issues) ---"
$PSQL "SELECT pg_size_pretty(pg_total_relation_size('issues')) AS total, pg_size_pretty(pg_indexes_size('issues')) AS indexes;"
echo "--- Existing index on (project_id)? ---"
$PSQL "SELECT indexname FROM pg_indexes WHERE tablename='issues';"
