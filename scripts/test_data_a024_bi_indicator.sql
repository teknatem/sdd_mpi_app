-- ============================================================
-- Тестовые данные: BI Индикаторы (a024_bi_indicator)
-- ============================================================
-- Запуск:
--   sqlite3 "F:/data/sdd_mpi_app/app.db" < scripts/test_data_a024_bi_indicator.sql
-- Или через API:
--   POST /api/a024-bi-indicator/testdata
-- ============================================================
-- Все записи используют INSERT OR IGNORE — безопасно запускать повторно.
-- owner_user_id = admin (f2fc6986-855d-492b-acff-70c7cd8cdd34)
-- ============================================================

-- 1. Выручка WB — KPI-карточка с большим числом и дельтой
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0001-4001-a001-000000000001',
    'IND-REVENUE-WB',
    'Выручка WB',
    'Тестовый индикатор: суммарная выручка Wildberries за выбранный период. Показывает главное число крупно, дельту зелёным/красным.',
    '{"schema_id":"","query_config":{"data_source":"","selected_fields":[],"groupings":[],"filters":{}},"sql_artifact_id":null}',
    '[{"key":"period","param_type":"date_range","label":"Период","default_value":null,"required":false,"global_filter_key":"date_range"}]',
    '{"custom_html":"<div class=\"kpi\"><div class=\"kpi__label\">{{title}}</div><div class=\"kpi__value\">{{value}}</div><div class=\"kpi__delta\">{{delta}}</div></div>","custom_css":".kpi{display:flex;flex-direction:column;gap:8px;height:100%;padding:4px}.kpi__label{font-size:11px;font-weight:600;color:var(--bi-text-secondary);text-transform:uppercase;letter-spacing:.6px}.kpi__value{font-size:2.4rem;font-weight:800;color:var(--bi-text);line-height:1}.kpi__delta{font-size:14px;font-weight:600;color:var(--bi-success);background:rgba(34,197,94,.12);padding:2px 10px;border-radius:12px;display:inline-block}","format":{"kind":"Money","currency":"RUB"},"thresholds":[]}',
    NULL,
    'active',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    1,
    datetime('now'),
    datetime('now'),
    1
);

-- 6. MP acquiring (fact) -- test indicator on universal GL turnover DataView dv004
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0016-4001-a001-000000000016',
    'IND-GL-MP-ACQ-FACT',
    'MP acquiring (fact)',
    'Test indicator on dv004_general_ledger_turnovers with turnover_code=mp_acquiring and layer=fact.',
    '{"view_id":"dv004_general_ledger_turnovers","metric_id":"amount"}',
    '[{"key":"turnover_code","param_type":"string","label":"Turnover code","default_value":"mp_acquiring","required":true,"global_filter_key":null},{"key":"layer","param_type":"string","label":"Layer","default_value":"fact","required":true,"global_filter_key":null}]',
    '{"style_name":"classic","custom_html":null,"custom_css":null,"format":{"kind":"Money","currency":"RUB"},"thresholds":[],"preview_values":{}}',
    NULL,
    'active',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    1,
    datetime('now'),
    datetime('now'),
    1
);

-- 7. MP penalty (fact) -- test indicator on universal GL turnover DataView dv004
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0017-4001-a001-000000000017',
    'IND-GL-MP-PENALTY-FACT',
    'MP penalty (fact)',
    'Test indicator on dv004_general_ledger_turnovers with turnover_code=mp_penalty and layer=fact.',
    '{"view_id":"dv004_general_ledger_turnovers","metric_id":"amount"}',
    '[{"key":"turnover_code","param_type":"string","label":"Turnover code","default_value":"mp_penalty","required":true,"global_filter_key":null},{"key":"layer","param_type":"string","label":"Layer","default_value":"fact","required":true,"global_filter_key":null}]',
    '{"style_name":"classic","custom_html":null,"custom_css":null,"format":{"kind":"Money","currency":"RUB"},"thresholds":[],"preview_values":{}}',
    NULL,
    'active',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    1,
    datetime('now'),
    datetime('now'),
    1
);

-- 8. MP logistics (fact) -- test indicator on universal GL turnover DataView dv004
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0018-4001-a001-000000000018',
    'IND-GL-MP-LOGISTICS-FACT',
    'MP logistics (fact)',
    'Test indicator on dv004_general_ledger_turnovers with turnover_code=mp_logistics and layer=fact.',
    '{"view_id":"dv004_general_ledger_turnovers","metric_id":"amount"}',
    '[{"key":"turnover_code","param_type":"string","label":"Turnover code","default_value":"mp_logistics","required":true,"global_filter_key":null},{"key":"layer","param_type":"string","label":"Layer","default_value":"fact","required":true,"global_filter_key":null}]',
    '{"style_name":"classic","custom_html":null,"custom_css":null,"format":{"kind":"Money","currency":"RUB"},"thresholds":[],"preview_values":{}}',
    NULL,
    'active',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    1,
    datetime('now'),
    datetime('now'),
    1
);

-- 2. Маржинальность — кольцеобразный индикатор процента + пороги
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0002-4001-a001-000000000002',
    'IND-MARGIN',
    'Маржинальность',
    'Тестовый индикатор: процент маржи. Кольцеобразный дизайн, пороговые значения: зелёный > 25%, красный < 10%.',
    '{"schema_id":"","query_config":{"data_source":"","selected_fields":[],"groupings":[],"filters":{}},"sql_artifact_id":null}',
    '[]',
    '{"custom_html":"<div class=\"ring-kpi\"><div class=\"ring-kpi__ring\"><span class=\"ring-kpi__num\">{{value}}</span></div><div class=\"ring-kpi__info\"><div class=\"ring-kpi__title\">{{title}}</div><div class=\"ring-kpi__delta\">{{delta}}</div></div></div>","custom_css":".ring-kpi{display:flex;align-items:center;gap:16px;height:100%}.ring-kpi__ring{width:76px;height:76px;border-radius:50%;border:6px solid var(--bi-primary);display:flex;align-items:center;justify-content:center;flex-shrink:0}.ring-kpi__num{font-size:1rem;font-weight:800;color:var(--bi-primary)}.ring-kpi__info{display:flex;flex-direction:column;gap:6px}.ring-kpi__title{font-size:11px;font-weight:600;color:var(--bi-text-secondary);text-transform:uppercase;letter-spacing:.5px}.ring-kpi__delta{font-size:14px;font-weight:600;color:var(--bi-success)}","format":{"kind":"Percent","decimals":1},"thresholds":[{"condition":"> 25","color":"rgb(34,197,94)","label":"Высокая"},{"condition":"< 10","color":"rgb(239,68,68)","label":"Низкая"}]}',
    NULL,
    'active',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    1,
    datetime('now'),
    datetime('now'),
    1
);

-- 3. Количество заказов — карточка с точкой-маркером и числом
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0003-4001-a001-000000000003',
    'IND-ORDERS',
    'Количество заказов',
    'Тестовый индикатор: суммарное количество заказов за период. Компактный дизайн с маркером-точкой.',
    '{"schema_id":"","query_config":{"data_source":"","selected_fields":[],"groupings":[],"filters":{}},"sql_artifact_id":null}',
    '[{"key":"period","param_type":"date_range","label":"Период","default_value":null,"required":false,"global_filter_key":"date_range"}]',
    '{"custom_html":"<div class=\"cnt-kpi\"><span class=\"cnt-kpi__dot\"></span><div class=\"cnt-kpi__body\"><div class=\"cnt-kpi__title\">{{title}}</div><div class=\"cnt-kpi__value\">{{value}}</div><div class=\"cnt-kpi__delta\">{{delta}}</div></div></div>","custom_css":".cnt-kpi{display:flex;align-items:flex-start;gap:12px;height:100%;padding:4px}.cnt-kpi__dot{width:10px;height:10px;border-radius:50%;background:var(--bi-primary);flex-shrink:0;margin-top:4px}.cnt-kpi__body{display:flex;flex-direction:column;gap:4px}.cnt-kpi__title{font-size:11px;font-weight:600;color:var(--bi-text-secondary);text-transform:uppercase;letter-spacing:.5px}.cnt-kpi__value{font-size:2.2rem;font-weight:800;color:var(--bi-text);line-height:1}.cnt-kpi__delta{font-size:13px;font-weight:600;color:var(--bi-success)}","format":{"kind":"Integer"},"thresholds":[]}',
    NULL,
    'active',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    1,
    datetime('now'),
    datetime('now'),
    1
);

-- 4. Выручка Ozon — дублирует дизайн #1, статус draft, для тестирования фильтрации по статусу
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0004-4001-a001-000000000004',
    'IND-REVENUE-OZON',
    'Выручка Ozon',
    'Тестовый индикатор (draft): выручка Ozon. Статус draft — для проверки фильтрации по статусу в списке.',
    '{"schema_id":"","query_config":{"data_source":"","selected_fields":[],"groupings":[],"filters":{}},"sql_artifact_id":null}',
    '[{"key":"period","param_type":"date_range","label":"Период","default_value":null,"required":false,"global_filter_key":"date_range"}]',
    '{"custom_html":"<div class=\"kpi\"><div class=\"kpi__label\">{{title}}</div><div class=\"kpi__value\">{{value}}</div><div class=\"kpi__delta\">{{delta}}</div></div>","custom_css":".kpi{display:flex;flex-direction:column;gap:8px;height:100%;padding:4px}.kpi__label{font-size:11px;font-weight:600;color:var(--bi-text-secondary);text-transform:uppercase;letter-spacing:.6px}.kpi__value{font-size:2.4rem;font-weight:800;color:var(--bi-text);line-height:1}.kpi__delta{font-size:14px;font-weight:600;color:var(--bi-success);background:rgba(34,197,94,.12);padding:2px 10px;border-radius:12px;display:inline-block}","format":{"kind":"Money","currency":"RUB"},"thresholds":[]}',
    NULL,
    'draft',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    0,
    datetime('now'),
    datetime('now'),
    1
);

-- 5. Пустой индикатор — без HTML/CSS, для тестирования empty-state в превью
INSERT OR IGNORE INTO a024_bi_indicator
    (id, code, description, comment, data_spec_json, params_json, view_spec_json,
     drill_spec_json, status, owner_user_id, is_public, created_at, updated_at, version)
VALUES (
    'a024a024-0005-4001-a001-000000000005',
    'IND-EMPTY',
    'Новый индикатор (без шаблона)',
    'Тестовый индикатор без HTML/CSS — для проверки empty-state подсказки на вкладке Превью.',
    '{"schema_id":"","query_config":{"data_source":"","selected_fields":[],"groupings":[],"filters":{}},"sql_artifact_id":null}',
    '[]',
    '{"custom_html":null,"custom_css":null,"format":{"kind":"Integer"},"thresholds":[]}',
    NULL,
    'draft',
    'f2fc6986-855d-492b-acff-70c7cd8cdd34',
    0,
    datetime('now'),
    datetime('now'),
    1
);
