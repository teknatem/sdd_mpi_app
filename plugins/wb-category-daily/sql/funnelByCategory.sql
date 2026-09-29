SELECT
    d.document_date AS day,
    COALESCE(NULLIF(TRIM(n.dim1_category), ''), 'Без категории') AS category,
    COALESCE(SUM(json_extract(j.value, '$.metrics.open_count')), 0) AS open_count,
    COALESCE(SUM(json_extract(j.value, '$.metrics.cart_count')), 0) AS cart_count,
    COALESCE(SUM(json_extract(j.value, '$.metrics.order_count')), 0) AS order_count
FROM a036_wb_sales_funnel_daily d, json_each(d.lines_json) j
LEFT JOIN a004_nomenclature n
  ON n.id = json_extract(j.value, '$.nomenclature_ref')
 AND n.is_deleted = 0
WHERE d.is_deleted = 0
  AND d.document_date >= ?
  AND d.document_date <= ?
  AND (? = '' OR d.connection_id = ?)
GROUP BY d.document_date, category
ORDER BY d.document_date, category
