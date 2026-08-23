// Этап st0003 «Закрыть день».
//
// Отдельного состояния «день закрыт» у a033 нет, и заводить его ради механизма
// мы не стали: домен не расширяется под процесс. Поэтому «закрыт» здесь —
// проверяемое утверждение, а не запись: снимок за день существует, он
// пересчитан и в нём нет блокирующих проблем.
//
// Ценность Этапа не в эффекте, а в том, что он спрашивает **другое**, чем
// st0002: тот смотрит на результат сверки, этот — на свежесть самого снимка.
// Пересборка могла тихо не состояться, и тогда «сходится» означало бы лишь то,
// что проблем не нашли во вчерашних данных.
export async function run(input, host) {
  const rows = await host.db.query(
    `SELECT COALESCE(d.last_recalculated_at, '') last_recalculated_at,
            COALESCE(d.snapshot_hash, '') snapshot_hash,
            COALESCE(json_extract(d.totals_json, '$.problems_block'), 0) problems_block
       FROM a033_wb_day_close d
      WHERE d.connection_id = ?
        AND d.business_date = ?
        AND d.is_archived = 0
        AND d.is_deleted = 0
      LIMIT 1`,
    [input.connection_id, input.business_date]
  );

  if (rows.length === 0) {
    return { outcome: "не закрыт", data: { reason: "снимка дня нет" } };
  }

  const recalculated = String(rows[0].last_recalculated_at || "");
  const hash = String(rows[0].snapshot_hash || "");
  const blocking = Number(rows[0].problems_block || 0);

  if (recalculated === "" || hash === "") {
    return { outcome: "не закрыт", data: { reason: "снимок не пересчитан" } };
  }
  if (blocking > 0) {
    return {
      outcome: "не закрыт",
      data: { reason: `блокирующих проблем: ${blocking}` }
    };
  }

  return {
    outcome: "закрыт",
    data: { last_recalculated_at: recalculated, snapshot_hash: hash }
  };
}
