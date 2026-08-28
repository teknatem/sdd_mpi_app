// Этап st0009 «Починить ссылки в проекциях».
export async function run(input, host) {
  const effect = await host.actions.repairEmptyNomenclatureRefs(
    { max_groups: 200 },
    { key: "nip" }
  );

  return {
    outcome: "перепроведено",
    data: {
      process_code: String(input.process_code || "pr0002"),
      reposted: Number(effect?.result?.reposted ?? 0),
      requested: Number(effect?.result?.requested ?? 0),
      groups: Number(effect?.result?.groups ?? 0),
      capped: Boolean(effect?.result?.capped)
    }
  };
}
