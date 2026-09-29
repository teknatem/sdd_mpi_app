WITH dbs_flag AS (
    SELECT
        srid,
        MAX(CASE WHEN srv_dbs = 1 THEN 1 ELSE 0 END) AS is_dbs
    FROM p903_wb_finance_report
    WHERE srid IS NOT NULL AND srid != ''
    GROUP BY srid
),
rows AS (
    SELECT
        substr(s.sale_date, 1, 10) AS day,
        CASE
            WHEN is_customer_return = 1
              OR LOWER(COALESCE(s.event_type, '')) IN ('return', 'возврат')
            THEN 1
            ELSE 0
        END AS is_ret,
        CASE
            WHEN COALESCE(d.is_dbs, 0) = 1
              OR json_extract(s.warehouse_json, '$.warehouse_type') = 'DBS'
            THEN 'DBS'
            WHEN json_extract(s.warehouse_json, '$.warehouse_type') IN ('DBW', 'FBW')
            THEN 'DBW'
            WHEN json_extract(s.warehouse_json, '$.warehouse_type') IN ('FBS', 'Склад продавца')
            THEN 'FBS'
            ELSE 'FBO'
        END AS scheme,
        ABS(COALESCE(s.qty, 0)) AS qty_abs,
        ABS(COALESCE(s.finished_price, 0)) AS price_abs,
        ABS(COALESCE(s.prod_cost_resolved_total, s.cost_of_production, 0)) AS cost_abs
    FROM a012_wb_sales s
    LEFT JOIN dbs_flag d ON d.srid = s.document_no
    WHERE s.is_deleted = 0
      AND s.sale_date IS NOT NULL
      AND s.sale_date >= ?
      AND s.sale_date <= ?
      AND (? = '' OR s.connection_id = ?)
)
SELECT
    day,
    COALESCE(SUM(CASE WHEN is_ret = 0 THEN qty_abs ELSE 0 END), 0) AS buyout_qty,
    COALESCE(SUM(CASE WHEN is_ret = 0 AND scheme = 'FBO' THEN qty_abs ELSE 0 END), 0) AS buyout_fbo,
    COALESCE(SUM(CASE WHEN is_ret = 0 AND scheme = 'FBS' THEN qty_abs ELSE 0 END), 0) AS buyout_fbs,
    COALESCE(SUM(CASE WHEN is_ret = 0 AND scheme = 'DBS' THEN qty_abs ELSE 0 END), 0) AS buyout_dbs,
    COALESCE(SUM(CASE WHEN is_ret = 0 AND scheme = 'DBW' THEN qty_abs ELSE 0 END), 0) AS buyout_dbw,
    COALESCE(SUM(CASE WHEN is_ret = 0 AND scheme IN ('DBS', 'DBW') THEN qty_abs ELSE 0 END), 0) AS buyout_dbs_dbw,
    COALESCE(SUM(CASE WHEN is_ret = 0 THEN price_abs ELSE 0 END), 0) AS revenue_rub,
    COALESCE(SUM(CASE WHEN is_ret = 1 THEN qty_abs ELSE 0 END), 0) AS return_qty,
    COALESCE(SUM(CASE WHEN is_ret = 1 THEN price_abs ELSE 0 END), 0) AS return_sum,
    COALESCE(SUM(CASE WHEN is_ret = 0 THEN cost_abs ELSE -cost_abs END), 0) AS cost_incl_returns
FROM rows
GROUP BY day
ORDER BY day
