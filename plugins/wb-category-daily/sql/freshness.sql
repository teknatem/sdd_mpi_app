SELECT MAX(document_date) AS last_date
FROM a036_wb_sales_funnel_daily
WHERE is_deleted = 0
  AND document_date IS NOT NULL
