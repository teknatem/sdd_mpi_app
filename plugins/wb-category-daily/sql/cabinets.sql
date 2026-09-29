SELECT DISTINCT
    c.id,
    c.description AS name
FROM a006_connection_mp c
INNER JOIN a036_wb_sales_funnel_daily d ON d.connection_id = c.id
WHERE c.is_deleted = 0
  AND d.is_deleted = 0
ORDER BY c.description
