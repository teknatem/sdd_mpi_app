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
        substr(o.document_date, 1, 10) AS day,
        CASE
            WHEN COALESCE(d.is_dbs, 0) = 1
              OR json_extract(o.warehouse_json, '$.warehouse_type') = 'DBS'
            THEN 'DBS'
            WHEN json_extract(o.warehouse_json, '$.warehouse_type') IN ('DBW', 'FBW')
            THEN 'DBW'
            WHEN json_extract(o.warehouse_json, '$.warehouse_type') IN ('FBS', 'Склад продавца')
            THEN 'FBS'
            ELSE 'FBO'
        END AS scheme,
        COALESCE(json_extract(o.line_json, '$.qty'), 1) AS qty,
        COALESCE(json_extract(o.line_json, '$.finished_price'), 0) AS finished_price
    FROM a015_wb_orders o
    LEFT JOIN dbs_flag d ON d.srid = o.document_no
    WHERE o.is_deleted = 0
      AND o.document_date IS NOT NULL
      AND o.document_date >= ?
      AND o.document_date <= ?
      AND (? = '' OR json_extract(o.header_json, '$.connection_id') = ?)
)
SELECT
    day,
    COALESCE(SUM(qty), 0) AS order_qty,
    COALESCE(SUM(CASE WHEN scheme = 'FBO' THEN qty ELSE 0 END), 0) AS order_fbo,
    COALESCE(SUM(CASE WHEN scheme = 'FBS' THEN qty ELSE 0 END), 0) AS order_fbs,
    COALESCE(SUM(CASE WHEN scheme = 'DBS' THEN qty ELSE 0 END), 0) AS order_dbs,
    COALESCE(SUM(CASE WHEN scheme = 'DBW' THEN qty ELSE 0 END), 0) AS order_dbw,
    COALESCE(SUM(CASE WHEN scheme IN ('DBS', 'DBW') THEN qty ELSE 0 END), 0) AS order_dbs_dbw,
    COALESCE(SUM(finished_price), 0) AS order_rub
FROM rows
GROUP BY day
ORDER BY day
