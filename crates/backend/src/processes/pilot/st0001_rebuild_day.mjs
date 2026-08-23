// Этап st0001 «Пересчитать день».
//
// Единственный Этап пилота, который сам меняет данные: пересобирает снимок
// закрытия дня (a033) из проекций. Всё остальное в pr0001 читает и решает.
//
// Ключ идемпотентности вызова несёт номер захода — его подставляет рантайм, а
// не этот код. Смысловой суффикс ниже нужен для другого: чтобы два разных дня
// внутри одного экземпляра (если такое когда-нибудь случится) не схлопнулись в
// один эффект.
export async function run(input, host) {
  const effect = await host.actions.rebuildDayClose(
    {
      connection_id: input.connection_id,
      business_date: input.business_date
    },
    { key: `${input.connection_id}:${input.business_date}` }
  );

  return {
    outcome: "пересчитан",
    data: {
      document_id: String(effect?.result?.document_id ?? ""),
      problems: Number(effect?.result?.problems ?? 0)
    }
  };
}
