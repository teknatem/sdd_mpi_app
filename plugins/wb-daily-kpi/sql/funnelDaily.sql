SELECT
    document_date AS day,
    COALESCE(SUM(total_open_count), 0) AS open_count,
    COALESCE(SUM(total_cart_count), 0) AS cart_count,
    COALESCE(SUM(total_order_count), 0) AS funnel_order_count
FROM a036_wb_sales_funnel_daily
WHERE is_deleted = 0
  AND document_date >= ?
  AND document_date <= ?
  AND (? = '' OR connection_id = ?)
GROUP BY document_date
ORDER BY document_date
