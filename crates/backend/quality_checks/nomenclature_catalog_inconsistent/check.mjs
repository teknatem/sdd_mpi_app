// Парная проверка Процесса pr0002 «Проверка номенклатуры» (ADR-0011 п.3, п.4).
//
// Нарушение каталога: активный a007 без однозначной связи с a004 —
// пустой nomenclature_ref ИЛИ артикул совпадает с несколькими позициями a004
// (u505 в этом случае очищает связь, поэтому неоднозначность видна повторным JOIN).

const DEFAULT_SAMPLE = 20;

export async function run(input, host) {
  const sampleLimit = Number(input?.sample_limit ?? DEFAULT_SAMPLE);

  const catalogRows = await host.db.query(
    `SELECT p.id AS product_id,
            p.article AS article,
            p.marketplace_sku AS marketplace_sku,
            p.connection_mp_ref AS connection_id,
            p.nomenclature_ref AS nomenclature_ref,
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
       ) hits
         ON hits.art_key = LOWER(TRIM(COALESCE(p.article, '')))
      WHERE p.is_deleted = 0
      ORDER BY p.article`,
    []
  );

  const violations = [];
  let population = 0;
  let unmatched = 0;
  let ambiguous = 0;

  for (const row of catalogRows) {
    population += 1;
    const refEmpty =
      row.nomenclature_ref == null ||
      String(row.nomenclature_ref).trim() === "";
    const hits = Number(row.hit_count || 0);
    const isAmbiguous = hits > 1;
    const isUnmatched = refEmpty && hits <= 1;

    if (!refEmpty && !isAmbiguous) continue;

    if (isAmbiguous) ambiguous += 1;
    else unmatched += 1;

    if (violations.length < sampleLimit) {
      const reason = isAmbiguous
        ? `артикул неоднозначен (${hits} позиций в a004)`
        : hits === 0
          ? "артикул не найден в a004"
          : "nomenclature_ref пуст";
      violations.push({
        violation_type: isAmbiguous ? "ambiguous_article" : "unmatched_product",
        projection_table: "a007_marketplace_product",
        projection_id: String(row.product_id || ""),
        detail: `артикул «${row.article || ""}», SKU ${row.marketplace_sku || ""}, кабинет ${row.connection_id || ""}: ${reason}`
      });
    }
  }

  const projectionMetrics = [];
  for (const table of [
    ["p909_mp_order_line_turnovers", "p909 — обороты строк заказов"],
    ["p911_wb_advert_by_items", "p911 — реклама WB по номенклатуре"],
    ["p913_wb_advert_order_attr", "p913 — атрибуция рекламы WB"]
  ]) {
    const [name, label] = table;
    const rows = await host.db.query(
      `SELECT CAST(COUNT(*) AS INTEGER) AS population,
              CAST(SUM(CASE WHEN nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '' THEN 1 ELSE 0 END) AS INTEGER) AS violations
         FROM ${name}`,
      []
    );
    const r = rows[0] || {};
    projectionMetrics.push({
      label: `${label}: пустой nomenclature_ref`,
      population: Number(r.population || 0),
      violations: Number(r.violations || 0),
      unit: "строк"
    });
  }

  return {
    metrics: [
      {
        label: "Товары МП без однозначной связи с 1С",
        population,
        violations: unmatched + ambiguous,
        unit: "товаров"
      },
      {
        label: "Несопоставлено (нет в 1С / пустая связь)",
        population,
        violations: unmatched,
        unit: "товаров"
      },
      {
        label: "Неоднозначный артикул",
        population,
        violations: ambiguous,
        unit: "товаров"
      },
      ...projectionMetrics
    ],
    violations
  };
}
