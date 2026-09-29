SELECT DISTINCT
    c.id,
    c.description AS name
FROM a006_connection_mp c
INNER JOIN a012_wb_sales s ON s.connection_id = c.id
WHERE c.is_deleted = 0
  AND s.is_deleted = 0
ORDER BY c.description
