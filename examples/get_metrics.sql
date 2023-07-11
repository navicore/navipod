SELECT 
  t1.subject AS UUID,
  t1.object AS k8p_metric_name,
  t2.object AS k8p_value
FROM 
  triples AS t1
INNER JOIN 
  triples AS t2
ON 
  t1.subject = t2.subject
WHERE 
  t1.predicate = 'k8p_metric_name'
AND 
  t2.predicate = 'k8p_value'
LIMIT 10;
