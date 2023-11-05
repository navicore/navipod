SELECT
  t1.subject AS UUID,
  t1.object AS navipod_metric_name,
  t2.object AS navipod_value
FROM
  triples AS t1
INNER JOIN
  triples AS t2
ON
  t1.subject = t2.subject
WHERE
  t1.predicate = 'navipod_metric_name'
AND
  t2.predicate = 'navipod_value'
LIMIT 10;

SELECT
  t1.object AS navipod_metric_name,
  t2.object AS navipod_value,
  t3.object AS navipod_appname,
  t4.object AS navipod_podname,
  t5.object AS navipod_datetime
FROM
  triples AS t1
INNER JOIN
  triples AS t2 ON t1.subject = t2.subject
INNER JOIN
  triples AS t3 ON t1.subject = t3.subject
INNER JOIN
  triples AS t4 ON t1.subject = t4.subject
INNER JOIN
  triples AS t5 ON t1.subject = t5.subject
WHERE
  t1.predicate = 'navipod_metric_name'
AND
  t2.predicate = 'navipod_value'
AND
  t3.predicate = 'navipod_appname'
AND
  t4.predicate = 'navipod_podname'
AND
  t5.predicate = 'navipod_datetime'
LIMIT 10;
