//! Demo plugin fixture used by `/api/plugin/testdata`.

use super::repository;
use chrono::Utc;
use contracts::plugins::{
    DataBinding, PluginBundle, PluginDefinition, PluginManifest, PluginRuntime, PluginStatus,
    ViewSpec,
};

fn db() -> &'static sea_orm::DatabaseConnection {
    crate::shared::data::db::get_connection()
}

const DEMO_CLIENT_SCRIPT: &str = r##"
function isoDate(date) {
  return date.toISOString().slice(0, 10);
}

function cell(value) {
  const td = document.createElement("td");
  td.textContent = value == null || value === "" ? "-" : String(value);
  return td;
}

export async function mount(root, host) {
  const today = new Date();
  const from = new Date(today);
  from.setDate(from.getDate() - 30);

  root.innerHTML = `
    <main class="report">
      <header class="report__header">
        <div>
          <h1>WB order positions without 1C nomenclature</h1>
          <p>Checks Wildberries ordered positions against 1C nomenclature links for the selected period.</p>
        </div>
      </header>
      <section class="filters">
        <label class="field">Date from <input id="date-from" type="date"></label>
        <label class="field">Date to <input id="date-to" type="date"></label>
        <button id="refresh" class="btn" type="button">Refresh</button>
      </section>
      <section class="stats">
        <div class="stat"><span class="stat__label">Period</span><strong class="stat__value" id="st-period">-</strong></div>
        <div class="stat"><span class="stat__label">Checked</span><strong class="stat__value" id="st-checked">-</strong></div>
        <div class="stat stat--ok"><span class="stat__label">Matched</span><strong class="stat__value" id="st-matched">-</strong></div>
        <div class="stat stat--bad"><span class="stat__label">Unmatched</span><strong class="stat__value" id="st-unmatched">-</strong></div>
      </section>
      <div id="status" class="status"></div>
      <div class="table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Connection</th>
              <th>Article</th>
              <th>WB SKU</th>
              <th>Product</th>
              <th class="num">Orders</th>
              <th class="num">Qty</th>
              <th>First order</th>
              <th>Last order</th>
            </tr>
          </thead>
          <tbody id="rows"></tbody>
        </table>
      </div>
    </main>`;

  const dateFrom = root.querySelector("#date-from");
  const dateTo = root.querySelector("#date-to");
  const refresh = root.querySelector("#refresh");
  const status = root.querySelector("#status");
  const period = root.querySelector("#st-period");
  const checked = root.querySelector("#st-checked");
  const matched = root.querySelector("#st-matched");
  const unmatched = root.querySelector("#st-unmatched");
  const tbody = root.querySelector("#rows");
  dateFrom.value = isoDate(from);
  dateTo.value = isoDate(today);

  async function load() {
    refresh.disabled = true;
    status.className = "status";
    status.textContent = "Loading...";
    tbody.replaceChildren();
    checked.textContent = "-";
    matched.textContent = "-";
    unmatched.textContent = "-";

    try {
      const report = await host.invoke("loadReport", {
        dateFrom: dateFrom.value,
        dateTo: dateTo.value
      });
      const summary = report.summary || {};
      const rows = report.rows || [];
      period.textContent = `${report.period.from} - ${report.period.to}`;
      checked.textContent = summary.checked ?? 0;
      matched.textContent = summary.matched ?? 0;
      unmatched.textContent = summary.unmatched ?? 0;

      if (rows.length === 0) {
        status.className = "status status--ok";
        status.textContent = "No unmatched positions for the selected period.";
        return;
      }

      status.textContent = `Unmatched positions: ${rows.length}`;
      for (const row of rows) {
        const tr = document.createElement("tr");
        tr.append(
          cell(row.connection_name),
          cell(row.article),
          cell(row.marketplace_sku),
          cell(row.product_name),
          cell(row.order_count),
          cell(row.ordered_qty),
          cell(row.first_order_date),
          cell(row.last_order_date)
        );
        tr.children[4].className = "num";
        tr.children[5].className = "num";
        tbody.append(tr);
      }
    } catch (error) {
      status.className = "status status--error";
      status.textContent = error instanceof Error ? error.message : String(error);
    } finally {
      refresh.disabled = false;
    }
  }

  refresh.addEventListener("click", load);
  await load();
}
"##;

const DEMO_SERVER_SCRIPT: &str = r#"
export async function loadReport(args, host) {
  if (!args.dateFrom || !args.dateTo) {
    throw new Error("Both date boundaries are required");
  }

  host.log.info("Checking WB order positions", args.dateFrom, args.dateTo);
  const summaryRows = await host.db.queryResource("orderPositionsSummary", [args.dateFrom, args.dateTo]);
  const rows = await host.db.queryResource("unmappedOrderPositions", [args.dateFrom, args.dateTo]);
  const summary = summaryRows[0] || { checked: 0, matched: 0, unmatched: 0 };
  host.log.info("Checked:", summary.checked, "unmatched:", summary.unmatched);
  return {
    period: { from: args.dateFrom, to: args.dateTo },
    summary,
    rows
  };
}
"#;

const DEMO_SUMMARY_SQL: &str = r#"
SELECT
  COUNT(*) AS checked,
  SUM(CASE WHEN pos.nomenclature_id IS NOT NULL THEN 1 ELSE 0 END) AS matched,
  SUM(CASE WHEN pos.nomenclature_id IS NULL THEN 1 ELSE 0 END) AS unmatched
FROM (
  SELECT MAX(n.id) AS nomenclature_id
  FROM p909_mp_order_line_turnovers p
  LEFT JOIN a007_marketplace_product mp
    ON mp.id = p.marketplace_product_ref
   AND mp.is_deleted = 0
  LEFT JOIN a004_nomenclature n
    ON n.id = mp.nomenclature_ref
   AND n.is_deleted = 0
  LEFT JOIN a006_connection_mp c
    ON c.id = p.connection_mp_ref
   AND c.is_deleted = 0
  WHERE p.turnover_code = 'qty_ordered'
    AND p.layer = 'oper'
    AND c.marketplace = 'WB'
    AND p.entry_date BETWEEN ? AND ?
  GROUP BY
    p.connection_mp_ref,
    COALESCE(NULLIF(p.marketplace_product_ref, ''), p.line_key)
) pos
"#;

const DEMO_REPORT_SQL: &str = r#"
SELECT
  COALESCE(c.description, p.connection_mp_ref) AS connection_name,
  COALESCE(NULLIF(mp.article, ''), p.line_key) AS article,
  COALESCE(NULLIF(mp.marketplace_sku, ''), p.line_key) AS marketplace_sku,
  COALESCE(NULLIF(mp.description, ''), '(product card is missing)') AS product_name,
  COUNT(DISTINCT p.order_key) AS order_count,
  ROUND(SUM(p.amount), 3) AS ordered_qty,
  MIN(p.entry_date) AS first_order_date,
  MAX(p.entry_date) AS last_order_date
FROM p909_mp_order_line_turnovers p
LEFT JOIN a007_marketplace_product mp
  ON mp.id = p.marketplace_product_ref
 AND mp.is_deleted = 0
LEFT JOIN a004_nomenclature n
  ON n.id = mp.nomenclature_ref
 AND n.is_deleted = 0
LEFT JOIN a006_connection_mp c
  ON c.id = p.connection_mp_ref
 AND c.is_deleted = 0
WHERE p.turnover_code = 'qty_ordered'
  AND p.layer = 'oper'
  AND c.marketplace = 'WB'
  AND p.entry_date BETWEEN ? AND ?
  AND n.id IS NULL
GROUP BY
  p.connection_mp_ref,
  COALESCE(NULLIF(p.marketplace_product_ref, ''), p.line_key),
  mp.article,
  mp.marketplace_sku,
  mp.description
ORDER BY order_count DESC, article
LIMIT 1000
"#;

const DEMO_STYLES: &str = r#"
.report { padding: 20px 24px; display: flex; flex-direction: column; gap: 14px; }
.report__header h1 { font-size: var(--font-size-lg); }
.table-wrap { max-height: 70vh; }
"#;

pub async fn insert_test_data() -> anyhow::Result<()> {
    use sea_orm::{ConnectionTrait, DbBackend, Statement};

    const TEST_ID: &str = "9f1c0a00-0000-4000-8000-000000000001";
    const TEST_CODE: &str = "PLG-WB-UNMAPPED-ORDERS";

    db().execute(Statement::from_sql_and_values(
        DbBackend::Sqlite,
        "DELETE FROM plugin WHERE code = ? OR id = ?",
        vec![TEST_CODE.into(), TEST_ID.into()],
    ))
    .await?;

    let now = Utc::now();
    let def = PluginDefinition {
        id: TEST_ID.to_string(),
        bundle: PluginBundle {
            manifest: PluginManifest {
                code: TEST_CODE.to_string(),
                title: "WB order positions without 1C nomenclature".to_string(),
                runtime: PluginRuntime::Hybrid,
                api_version: "2".to_string(),
                description: Some(
                    "Demo JavaScript plugin: iframe UI calls a server ES module via host.invoke()."
                        .to_string(),
                ),
                capabilities: vec!["data:read".to_string()],
                // Рисует собственный HTML классами plugin-sdk.css: ни один
                // глобал кита не используется.
                client_kits: Some(vec![]),
                built_for_migration: None,
            },
            params: vec![],
            data: DataBinding::default(),
            client_script: Some(DEMO_CLIENT_SCRIPT.to_string()),
            server_script: Some(DEMO_SERVER_SCRIPT.to_string()),
            view_spec: ViewSpec::default(),
            styles: Some(DEMO_STYLES.to_string()),
            sql_resources: [
                (
                    "unmappedOrderPositions".to_string(),
                    DEMO_REPORT_SQL.trim().to_string(),
                ),
                (
                    "orderPositionsSummary".to_string(),
                    DEMO_SUMMARY_SQL.trim().to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            assets: Default::default(),
        },
        status: PluginStatus::Active,
        is_enabled: true,
        owner_user_id: None,
        created_by_agent_id: None,
        version: 1,
        created_at: now,
        updated_at: now,
        rating: None,
        snapshot: None,
        s3_published_version: None,
        s3_published_at: None,
    };
    repository::insert(db(), &def).await?;
    Ok(())
}
