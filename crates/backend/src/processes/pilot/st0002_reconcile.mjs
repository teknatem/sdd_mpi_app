// Этап st0002 «Сверить день».
//
// Сверка дня — это блокирующие проблемы снимка a033: его детекторы и есть
// сверка (реклама без списания, непроведённый a012 при строках p903,
// нарушение инварианта колонок и так далее). Отдельного «сравнить с ГК одним
// числом» в домене нет, и выдумывать его Этап не имеет права: он читает то,
// что домен уже посчитал.
//
// Эффектов нет — Этап только решает, куда пойдёт процесс.
export async function run(input, host) {
  const rows = await host.db.query(
    `SELECT COALESCE(json_extract(d.totals_json, '$.problems_block'), 0) problems_block,
            COALESCE(json_extract(d.totals_json, '$.problems_warn'), 0) problems_warn,
            COALESCE(d.last_recalculated_at, '') last_recalculated_at
       FROM a033_wb_day_close d
      WHERE d.connection_id = ?
        AND d.business_date = ?
        AND d.is_archived = 0
        AND d.is_deleted = 0
      LIMIT 1`,
    [input.connection_id, input.business_date]
  );

  // Снимка нет вовсе — это расхождение, а не «сходится»: сверять нечего.
  if (rows.length === 0) {
    return {
      outcome: "расхождение",
      data: { problems_block: 0, problems_warn: 0, reason: "снимка дня нет" }
    };
  }

  const blocking = Number(rows[0].problems_block || 0);
  const warnings = Number(rows[0].problems_warn || 0);
  if (blocking === 0) {
    return { outcome: "сходится", data: { problems_warn: warnings } };
  }

  return {
    outcome: "расхождение",
    data: {
      problems_block: blocking,
      problems_warn: warnings,
      reason: `блокирующих проблем: ${blocking}`
    }
  };
}
