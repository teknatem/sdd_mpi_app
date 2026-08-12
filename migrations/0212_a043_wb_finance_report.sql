-- a043: новый независимый агрегат финансовых отчётов WB Finance API v1.
-- Legacy p903/task006 и его проводки эта миграция не изменяет.
CREATE TABLE IF NOT EXISTS a043_wb_finance_report (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL DEFAULT '',
    description TEXT NOT NULL DEFAULT '',
    comment TEXT,
    document_no TEXT NOT NULL DEFAULT '',
    document_date TEXT NOT NULL DEFAULT '',
    connection_id TEXT NOT NULL,
    organization_id TEXT NOT NULL DEFAULT '',
    marketplace_id TEXT NOT NULL DEFAULT '',
    report_id TEXT NOT NULL,
    period TEXT NOT NULL DEFAULT 'daily',
    date_from TEXT NOT NULL DEFAULT '',
    date_to TEXT NOT NULL DEFAULT '',
    create_date TEXT NOT NULL DEFAULT '',
    seller_finance_name TEXT NOT NULL DEFAULT '',
    currency TEXT NOT NULL DEFAULT '',
    report_type INTEGER,
    retail_amount_sum TEXT,
    for_pay_sum TEXT,
    delivery_service_sum TEXT,
    paid_storage_sum TEXT,
    paid_acceptance_sum TEXT,
    deduction_sum TEXT,
    penalty_sum TEXT,
    additional_payment_sum TEXT,
    cashback_amount_sum TEXT,
    cashback_discount_sum TEXT,
    cashback_commission_change_sum TEXT,
    payment_schedule TEXT,
    bank_payment_sum TEXT,
    lines_count INTEGER NOT NULL DEFAULT 0,
    pages_count INTEGER NOT NULL DEFAULT 0,
    last_rrd_id TEXT,
    header_json TEXT NOT NULL DEFAULT '{}',
    lines_json TEXT NOT NULL DEFAULT '[]',
    source_meta_json TEXT NOT NULL DEFAULT '{}',
    fetched_at TEXT NOT NULL DEFAULT '',
    is_deleted INTEGER NOT NULL DEFAULT 0,
    created_at TEXT,
    updated_at TEXT,
    version INTEGER NOT NULL DEFAULT 1
);

CREATE UNIQUE INDEX IF NOT EXISTS ux_a043_connection_period_report
    ON a043_wb_finance_report(connection_id, period, report_id)
    WHERE is_deleted = 0;
CREATE INDEX IF NOT EXISTS ix_a043_document_date ON a043_wb_finance_report(document_date);
CREATE INDEX IF NOT EXISTS ix_a043_period_dates ON a043_wb_finance_report(connection_id, date_from, date_to);

INSERT OR IGNORE INTO sys_role_scope_access (role_id, access_scope_id, access_mode)
SELECT id, 'a043_wb_finance_report', 'all' FROM sys_roles WHERE code IN ('manager', 'operator');
INSERT OR IGNORE INTO sys_role_scope_access (role_id, access_scope_id, access_mode)
SELECT id, 'a043_wb_finance_report', 'read' FROM sys_roles WHERE code = 'viewer';
