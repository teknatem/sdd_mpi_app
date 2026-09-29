SELECT
    document_date AS day,
    COALESCE(SUM(total_views), 0) AS views,
    COALESCE(SUM(total_clicks), 0) AS clicks,
    COALESCE(SUM(total_sum), 0) AS ad_spend,
    COALESCE(SUM(COALESCE(json_extract(totals_json, '$.atbs'), 0)), 0) AS atbs
FROM a026_wb_advert_daily
WHERE is_deleted = 0
  AND document_date >= ?
  AND document_date <= ?
  AND (? = '' OR connection_id = ?)
GROUP BY document_date
ORDER BY document_date
