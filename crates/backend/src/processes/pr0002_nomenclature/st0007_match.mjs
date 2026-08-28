// Этап st0007 «Сопоставить».
export async function run(input, host) {
  const effect = await host.actions.matchNomenclature(
    {
      overwrite_existing: false,
      ignore_case: true
    },
    { key: "all" }
  );

  return {
    outcome: "сопоставлено",
    data: {
      process_code: String(input.process_code || "pr0002"),
      matched: Number(effect?.result?.matched ?? 0),
      cleared: Number(effect?.result?.cleared ?? 0),
      ambiguous: Number(effect?.result?.ambiguous ?? 0),
      skipped: Number(effect?.result?.skipped ?? 0),
      errors: Number(effect?.result?.errors ?? 0)
    }
  };
}
