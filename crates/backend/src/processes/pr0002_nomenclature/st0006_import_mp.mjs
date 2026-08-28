// Этап st0006 «Подтянуть товары площадок».
//
// Обходит активные кабинеты a006 и для каждого зовёт importMarketplaceProducts.
export async function run(input, host) {
  const cabinets = await host.db.query(
    `SELECT id FROM a006_connection_mp
      WHERE is_deleted = 0
        AND COALESCE(is_used, 1) = 1
      ORDER BY code ASC`,
    []
  );

  const imported = [];
  const skipped = [];
  for (const row of cabinets) {
    const id = String(row.id || "");
    if (!id) continue;
    try {
      const effect = await host.actions.importMarketplaceProducts(
        { connection_id: id },
        { key: id }
      );
      imported.push({
        connection_id: id,
        marketplace_code: String(effect?.result?.marketplace_code ?? ""),
        session_id: String(effect?.result?.session_id ?? "")
      });
    } catch (err) {
      // Неподдерживаемая площадка или сбой одного кабинета не должны валить
      // весь проход: остальное продолжаем, а ошибку отдаём в data.
      skipped.push({
        connection_id: id,
        error: String(err?.message || err)
      });
    }
  }

  return {
    outcome: "подтянуто",
    data: {
      process_code: String(input.process_code || "pr0002"),
      cabinets: imported.length,
      imported,
      skipped
    }
  };
}
