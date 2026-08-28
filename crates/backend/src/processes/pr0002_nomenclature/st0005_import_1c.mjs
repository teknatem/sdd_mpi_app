// Этап st0005 «Подтянуть номенклатуру 1С».
//
// Обходит активные подключения a001 и для каждого зовёт importNomenclature.
// Вход — process_code из process.due; список подключений читается из БД.
export async function run(input, host) {
  const connections = await host.db.query(
    `SELECT id FROM a001_connection_1c_database
      WHERE is_deleted = 0
      ORDER BY is_primary DESC, code ASC`,
    []
  );

  const imported = [];
  for (const row of connections) {
    const id = String(row.id || "");
    if (!id) continue;
    const effect = await host.actions.importNomenclature(
      { connection_1c_id: id, include_barcodes: true },
      { key: id }
    );
    imported.push({
      connection_1c_id: id,
      session_id: String(effect?.result?.session_id ?? "")
    });
  }

  return {
    outcome: "подтянуто",
    data: {
      process_code: String(input.process_code || "pr0002"),
      connections: imported.length,
      imported
    }
  };
}
