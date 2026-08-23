// Этап st0004 «Позвать человека».
//
// Эффект Этапа — только просьба (тикет). Само ожидание задаёт ребро графа:
// после этого выхода экземпляр встаёт в `waiting` и просыпается по событию
// `human.action.done` с тем же ключом, который ушёл в тикет.
export async function run(input, host) {
  const reason = String(input.reason || "день не сходится");
  const effect = await host.actions.requestHumanAction(
    {
      title: `Разобрать день ${input.business_date}`,
      request_text:
        `Закрытие дня WB ${input.business_date} (кабинет ${input.connection_id}) остановлено: ${reason}. ` +
        `Разберите причину и отметьте «сделано» — процесс пересчитает день заново.`
    },
    { key: `${input.connection_id}:${input.business_date}` }
  );

  return {
    outcome: "позвали",
    data: {
      ticket_code: String(effect?.result?.code ?? ""),
      request_key: String(effect?.result?.request_key ?? "")
    }
  };
}
