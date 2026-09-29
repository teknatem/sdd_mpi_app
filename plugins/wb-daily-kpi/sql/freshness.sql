SELECT MAX(substr(sale_date, 1, 10)) AS last_date
FROM a012_wb_sales
WHERE is_deleted = 0
  AND sale_date IS NOT NULL
