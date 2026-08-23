// Парная проверка Процесса pr0001 «Закрытие дня WB» (ADR-0011 п.3, п.4).
//
// Смысл её не в том, чтобы дублировать процесс, а в том, чтобы рассогласование
// оставалось видимым, пока он до него не дошёл: экземпляр в середине графа —
// это состояние механизма, а незакрытый день — состояние учёта.
//
// Что считается закрытым днём: есть активный снимок a033 за (кабинет, день),
// он пересчитан (last_recalculated_at не пуст) и в нём нет блокирующих проблем
// (totals.problems_block = 0). Отдельного признака «закрыт» у a033 нет — это
// осознанное решение: заводить состояние в домене ради процесса значило бы
// расширять домен под механизм, а не наоборот.

const DEFAULT_GRACE_DAYS = 2;
const DEFAULT_WINDOW_DAYS = 60;
const SAMPLE_LIMIT = 20;

export async function run(input, host) {
  const grace = Number(input?.grace_days ?? DEFAULT_GRACE_DAYS);
  const window = Number(input?.window_days ?? DEFAULT_WINDOW_DAYS);

  // Границы окна считаем в JS, а не в SQL: 'now' в SQLite — это UTC, и на
  // границе суток проверка молча меняла бы популяцию.
  const day = 24 * 60 * 60 * 1000;
  const until = new Date(Date.now() - grace * day).toISOString().slice(0, 10);
  const since = new Date(Date.now() - window * day).toISOString().slice(0, 10);

  // Популяция — дни, по которым вообще были продажи: закрывать нечего там, где
  // ничего не продано.
  const rows = await host.db.query(
    `SELECT s.connection_id connection_id,
            substr(s.sale_date, 1, 10) business_date,
            CAST(COUNT(*) AS INTEGER) sales,
            CAST(COALESCE(MAX(CASE WHEN d.id IS NOT NULL THEN 1 ELSE 0 END), 0) AS INTEGER) has_snapshot,
            CAST(COALESCE(MAX(CASE WHEN d.last_recalculated_at IS NOT NULL AND d.last_recalculated_at <> '' THEN 1 ELSE 0 END), 0) AS INTEGER) recalculated,
            CAST(COALESCE(MAX(COALESCE(json_extract(d.totals_json, '$.problems_block'), 0)), 0) AS INTEGER) problems_block
       FROM a012_wb_sales s
       LEFT JOIN a033_wb_day_close d
              ON d.connection_id = s.connection_id
             AND d.business_date = substr(s.sale_date, 1, 10)
             AND d.is_archived = 0
             AND d.is_deleted = 0
      WHERE s.is_deleted = 0
        AND s.sale_date IS NOT NULL
        AND substr(s.sale_date, 1, 10) >= ?
        AND substr(s.sale_date, 1, 10) <= ?
      GROUP BY s.connection_id, substr(s.sale_date, 1, 10)
      ORDER BY business_date DESC`,
    [since, until]
  );

  const violations = [];
  let population = 0;
  let notClosed = 0;
  for (const row of rows) {
    population += 1;
    const closed =
      Number(row.has_snapshot || 0) === 1 &&
      Number(row.recalculated || 0) === 1 &&
      Number(row.problems_block || 0) === 0;
    if (closed) continue;

    notClosed += 1;
    if (violations.length < SAMPLE_LIMIT) {
      const reason =
        Number(row.has_snapshot || 0) !== 1
          ? "снимка дня нет"
          : Number(row.recalculated || 0) !== 1
            ? "снимок не пересчитан"
            : `блокирующих проблем: ${Number(row.problems_block || 0)}`;
      violations.push({
        violation_type: "day_not_closed",
        projection_table: "a033_wb_day_close",
        detail: `${row.business_date}, кабинет ${row.connection_id}: ${reason}; продаж за день ${row.sales}`
      });
    }
  }

  return {
    metrics: [
      {
        label: `Дни WB без закрытия (старше ${grace} дн.)`,
        population,
        violations: notClosed,
        unit: "дней"
      }
    ],
    violations
  };
}
