// Этап st0008 «Оценить».
//
// Решает, куда идти: чисто / только_проекции / остаток.
// Неоднозначность vs «нет в 1С» восстанавливается JOIN по артикулу — u505
// при неоднозначности очищает nomenclature_ref.
export async function run(input, host) {
  const alreadyReposted = Number(input.reposted ?? 0) > 0;

  const catalog = await host.db.query(
    `SELECT
        CAST(SUM(CASE
          WHEN (p.nomenclature_ref IS NULL OR TRIM(p.nomenclature_ref) = '')
           AND COALESCE(hits.hit_count, 0) <= 1 THEN 1 ELSE 0 END) AS INTEGER) AS unmatched,
        CAST(SUM(CASE
          WHEN COALESCE(hits.hit_count, 0) > 1 THEN 1 ELSE 0 END) AS INTEGER) AS ambiguous,
        CAST(COUNT(*) AS INTEGER) AS total
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
      WHERE p.is_deleted = 0`,
    []
  );

  const unmatched = Number(catalog[0]?.unmatched || 0);
  const ambiguous = Number(catalog[0]?.ambiguous || 0);
  const total = Number(catalog[0]?.total || 0);

  const projections = await host.db.query(
    `SELECT CAST(SUM(v) AS INTEGER) AS missing FROM (
        SELECT SUM(CASE WHEN nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '' THEN 1 ELSE 0 END) AS v
          FROM p909_mp_order_line_turnovers
        UNION ALL
        SELECT SUM(CASE WHEN nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '' THEN 1 ELSE 0 END)
          FROM p911_wb_advert_by_items
        UNION ALL
        SELECT SUM(CASE WHEN nomenclature_ref IS NULL OR TRIM(nomenclature_ref) = '' THEN 1 ELSE 0 END)
          FROM p913_wb_advert_order_attr
     )`,
    []
  );
  const projectionMissing = Number(projections[0]?.missing || 0);

  const summary = {
    process_code: String(input.process_code || "pr0002"),
    catalog_total: total,
    unmatched,
    ambiguous,
    projection_missing: projectionMissing,
    already_reposted: alreadyReposted
  };

  if (unmatched === 0 && ambiguous === 0 && projectionMissing === 0) {
    return { outcome: "чисто", data: summary };
  }

  // После одного перепроведения проекционные дырки — уже «остаток»: иначе цикл.
  if (unmatched === 0 && ambiguous === 0 && projectionMissing > 0 && !alreadyReposted) {
    return { outcome: "только_проекции", data: summary };
  }

  return {
    outcome: "остаток",
    data: {
      ...summary,
      reason:
        unmatched > 0 || ambiguous > 0
          ? `несопоставлено ${unmatched}, неоднозначно ${ambiguous}, дырок в проекциях ${projectionMissing}`
          : `после перепроведения остались дырки в проекциях: ${projectionMissing}`
    }
  };
}
