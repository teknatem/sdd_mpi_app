// Сборка кита flow: React + @xyflow/react + dagre + обёртка → один IIFE.
//
// Отдельный файл, а не строка флагов в package.json (как у plugin-editor),
// по одной причине: `--define:process.env.NODE_ENV='"production"'` не переживает
// разбор оболочки одинаково в cmd.exe и sh — кавычки съедаются, esbuild считает
// значение идентификатором, и в бандл молча уезжает dev-сборка React (810 КБ
// против 418 КБ). Через JS API значение передаётся как есть.
//
// Здесь же — потолок размера. Кит грузится в каждый iframe, объявивший "flow",
// и парсится заново на каждый документ; молчаливое разрастание тут никто не
// заметит, поэтому сборка падает, а не предупреждает.
//
// PREACT=1 подменяет react/react-dom на preact/compat: измерено 252.7 КБ против
// 418.2 КБ, то есть −40%. По умолчанию НЕ включено: xyflow не заявляет
// поддержку preact, и расхождения в реализации хуков всплыли бы на редакторе
// графов — поверхности, где цена тихого бага выше сэкономленных килобайт.
// Флаг оставлен, чтобы решение можно было пересмотреть с числами в руках.

import { build } from "esbuild";
import { statSync } from "node:fs";

const usePreact = process.env.PREACT === "1";

// React-сборка меряна: 418.2 КБ. Потолок — с запасом на мелкие добавления,
// но так, чтобы возврат dev-сборки React (810 КБ) обрушил сборку сразу.
const MAX_JS_BYTES = (usePreact ? 300 : 450) * 1024;

const result = await build({
  entryPoints: ["crates/frontend/js/plugin-flow.jsx"],
  outfile: "crates/frontend/static/plugin-flow.js",
  bundle: true,
  minify: true,
  format: "iife",
  target: "es2020",
  jsx: "automatic",
  legalComments: "none",
  define: { "process.env.NODE_ENV": '"production"' },
  ...(usePreact
    ? { alias: { react: "preact/compat", "react-dom": "preact/compat" } }
    : {}),
  metafile: true,
});

const js = statSync("crates/frontend/static/plugin-flow.js").size;
const css = statSync("crates/frontend/static/plugin-flow.css").size;
const kb = (bytes) => (bytes / 1024).toFixed(1) + " KB";

console.log(`${usePreact ? "preact/compat" : "react"}`);
console.log(`plugin-flow.js  ${kb(js)}`);
console.log(`plugin-flow.css ${kb(css)}`);

if (process.argv.includes("--why")) {
  const output = result.metafile.outputs["crates/frontend/static/plugin-flow.js"];
  const top = Object.entries(output.inputs)
    .sort((a, b) => b[1].bytesInOutput - a[1].bytesInOutput)
    .slice(0, 12);
  console.log("\nКрупнейшие входы:");
  for (const [path, info] of top) {
    console.log(`  ${kb(info.bytesInOutput).padStart(10)}  ${path}`);
  }
}

if (js > MAX_JS_BYTES) {
  console.error(
    `\nБюджет превышен: ${kb(js)} > ${kb(MAX_JS_BYTES)}.\n` +
      `Запусти с --why, чтобы увидеть, что раздулось.`
  );
  process.exit(1);
}
