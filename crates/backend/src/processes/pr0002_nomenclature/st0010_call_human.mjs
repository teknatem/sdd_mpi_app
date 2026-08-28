// Этап st0010 «Позвать человека».
//
// Один тикет на проход: сводка + выборка проблемных строк a007.
export async function run(input, host) {
  const unmatched = Number(input.unmatched || 0);
  const ambiguous = Number(input.ambiguous || 0);
  const projectionMissing = Number(input.projection_missing || 0);
  const reason = String(input.reason || "каталог номенклатуры не согласован");

  const samples = await host.db.query(
    `SELECT p.article AS article,
            p.marketplace_sku AS marketplace_sku,
            p.connection_mp_ref AS connection_id,
            CAST(COALESCE(hits.hit_count, 0) AS INTEGER) AS hit_count
       FROM a007_marketplace_product p
       LEFT JOIN (
         SELECT LOWER(TRIM(n.article)) AS art_key,
                CAST(COUNT(*) AS INTEGER) AS hit_count
           FROM a004_nomenclature n
          WHERE n.is_deleted = 0
            AND COALESCE(n.is_folder, 0) = 0
            AND TRIM(COALESCE(n.article, '')) <> ''
          GROUP BY LOWER(TRIM(n.article))
       ) hits ON hits.art_key = LOWER(TRIM(COALESCE(p.article, '')))
      WHERE p.is_deleted = 0
        AND (
          (p.nomenclature_ref IS NULL OR TRIM(p.nomenclature_ref) = '')
          OR COALESCE(hits.hit_count, 0) > 1
        )
      ORDER BY p.article
      LIMIT 30`,
    []
  );

  const lines = samples.map((row) => {
    const hits = Number(row.hit_count || 0);
    const kind =
      hits > 1
        ? `неоднозначно (${hits} в a004)`
        : hits === 0
          ? "нет в a004 — заведите номенклатуру в 1С или поправьте артикул"
          : "пустая связь — проверьте артикул";
    return `• «${row.article || ""}» / SKU ${row.marketplace_sku || ""} / кабинет ${row.connection_id || ""}: ${kind}`;
  });

  const more =
    unmatched + ambiguous > samples.length
      ? `\n… и ещё ${(unmatched + ambiguous) - samples.length} позиций (см. QC nomenclature_catalog_inconsistent)`
      : "";

  const requestText =
    `Проверка номенклатуры (pr0002) остановилась: ${reason}.\n\n` +
    `Сводка: несопоставлено ${unmatched}, неоднозначно ${ambiguous}, ` +
    `пустых nomenclature_ref в проекциях ${projectionMissing}.\n\n` +
    `Что сделать вручную (в 1С или на площадке — MPI сам туда не пишет):\n` +
    (lines.length ? lines.join("\n") + more : "• выборка пуста — смотрите QC") +
    `\n\nПосле правок отметьте «сделано» — процесс начнёт заново.`;

  const effect = await host.actions.requestHumanAction(
    {
      title: "Разобрать несоответствия номенклатуры",
      request_text: requestText
    },
    { key: "nomenclature" }
  );

  return {
    outcome: "позвали",
    data: {
      ticket_code: String(effect?.result?.code ?? ""),
      request_key: String(effect?.result?.request_key ?? ""),
      unmatched,
      ambiguous,
      projection_missing: projectionMissing
    }
  };
}
