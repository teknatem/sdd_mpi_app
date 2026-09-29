function formatNumber(value, fractionDigits) {
  const parsed = Number(value);
  const safe = Number.isFinite(parsed) ? parsed : 0;
  const parts = Math.abs(safe).toFixed(fractionDigits).split(".");
  const whole = parts[0].replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
  const sign = safe < 0 ? "−" : "";
  return parts.length === 2 ? `${sign}${whole},${parts[1]}` : `${sign}${whole}`;
}

function humanDate(value) {
  if (!value || value.length < 10) return value || "—";
  return `${value.slice(8, 10)}.${value.slice(5, 7)}.${value.slice(0, 4)}`;
}

function currentMonth() {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}`;
}

function monthPeriod(month) {
  const parts = String(month || "").split("-");
  const year = Number(parts[0]);
  const monthIndex = Number(parts[1]);
  if (!year || !monthIndex) throw new Error("Выберите месяц");
  const lastDay = new Date(year, monthIndex, 0).getDate();
  return {
    dateFrom: `${year}-${String(monthIndex).padStart(2, "0")}-01`,
    dateTo: `${year}-${String(monthIndex).padStart(2, "0")}-${String(lastDay).padStart(2, "0")}`
  };
}

function todayIso() {
  const now = new Date();
  return `${now.getFullYear()}-${String(now.getMonth() + 1).padStart(2, "0")}-${String(now.getDate()).padStart(2, "0")}`;
}

const GROUPS = [
  { id: "plan", title: "План", cols: [
    { key: "plan_rub", label: "Выручка план руб", type: "money" },
    { key: "forecast_rub", label: "Выручка прогноз", type: "money" }
  ]},
  { id: "buyout", title: "Выкупы", cols: [
    { key: "buyout_qty", label: "Выкупы шт", type: "int" },
    { key: "buyout_fbo", label: "Выкупы FBO шт", type: "int" },
    { key: "buyout_fbs", label: "Выкупы FBS шт", type: "int" },
    { key: "buyout_dbs_dbw", label: "Выкупы DBS/DBW шт", type: "int" },
    { key: "revenue_rub", label: "Выручка руб", type: "money" },
    { key: "buyout_avg", label: "Выкуп ср. выручка", type: "money" }
  ]},
  { id: "orders", title: "Заказы и маржа", cols: [
    { key: "order_qty", label: "Заказы шт", type: "int" },
    { key: "order_fbo", label: "Заказы FBO шт", type: "int" },
    { key: "order_fbs", label: "Заказы FBS шт", type: "int" },
    { key: "order_dbs_dbw", label: "Заказы DBS/DBW шт", type: "int" },
    { key: "order_rub", label: "Заказы руб", type: "money" },
    { key: "order_avg_check", label: "Заказы средний чек руб", type: "money" },
    { key: "gross_profit", label: "Валовая прибыль с уч возвратов", type: "money" },
    { key: "gross_margin", label: "Валовая рентабельность с уч возвратов", type: "pct" },
    { key: "cost_incl_returns", label: "Себестоимость с уч возвратов", type: "money" },
    { key: "return_qty", label: "Возвраты шт", type: "int" },
    { key: "return_sum", label: "Сумма возвратов руб", type: "money" }
  ]},
  { id: "ads", title: "Реклама", cols: [
    { key: "views", label: "Показы", type: "int" },
    { key: "clicks", label: "Клики", type: "int" },
    { key: "cpc", label: "CPC", type: "money" },
    { key: "ad_spend", label: "Рекл расходы руб", type: "money" },
    { key: "ctr", label: "CTR", type: "pct" },
    { key: "cr_cart", label: "CR в корзину", type: "pct" },
    { key: "cr_cart_order", label: "CR корз-заказ", type: "pct" },
    { key: "drr", label: "ДРР", type: "pct" }
  ]},
  { id: "cps", title: "CPS", cols: [
    { key: "cps", label: "CPS", type: "money" }
  ]}
];

function fmt(type, value) {
  if (value == null || value === "") return type === "int" ? "0" : "0,00";
  if (type === "int") return formatNumber(value, 0);
  if (type === "pct") return `${formatNumber(value, 2)}%`;
  return formatNumber(value, 2);
}

function excelCell(type, value) {
  if (type === "text") {
    let text = value == null ? "" : String(value);
    if (/^[=+\-@]/.test(text)) text = `'${text}`;
    return text.replace(/\t/g, " ").replace(/\r?\n/g, " ");
  }
  const parsed = Number(value);
  const safe = Number.isFinite(parsed) ? parsed : 0;
  const digits = type === "int" ? 0 : 2;
  return safe.toFixed(digits).replace(".", ",");
}

function excelRows(payload) {
  const groupRow = [""].concat(GROUPS.flatMap((group) => group.cols.map(() => group.title)));
  const headerRow = ["Дата"].concat(GROUPS.flatMap((group) => group.cols.map((col) => col.label)));
  const rows = [groupRow, headerRow];
  const data = (payload.rows || []).concat(payload.totals ? [payload.totals] : []);
  for (const row of data) {
    const cells = [row.day === "ИТОГО" ? "ИТОГО" : humanDate(row.day)];
    for (const group of GROUPS) {
      for (const col of group.cols) {
        cells.push(excelCell(col.type === "pct" ? "money" : col.type, row[col.key]));
      }
    }
    rows.push(cells);
  }
  return rows;
}

function buildExcelTsv(payload) {
  return excelRows(payload).map((row) => row.join("\t")).join("\r\n");
}

function csvCell(value) {
  let text = value == null ? "" : String(value);
  if (/^[=+\-@]/.test(text)) text = `'${text}`;
  if (/[";\n\r]/.test(text)) return `"${text.replace(/"/g, '""')}"`;
  return text;
}

function buildCsv(payload) {
  return excelRows(payload).map((row) => row.map(csvCell).join(";")).join("\r\n");
}

function downloadCsv(filename, csv) {
  const blob = new Blob(["\uFEFF" + csv], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

function csvFilename(month, title) {
  const raw = `${title} ${month || ""}`.replace(/[^\w\-а-яё ]+/gi, " ").replace(/\s+/g, " ").trim();
  return `${raw.slice(0, 60)}.csv`;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}

function buildExcelHtml(payload) {
  const rows = excelRows(payload);
  const body = rows.map((row, index) => {
    const tag = index < 2 ? "th" : "td";
    const cells = row.map((cell) => `<${tag}>${escapeHtml(cell)}</${tag}>`).join("");
    return `<tr>${cells}</tr>`;
  }).join("");
  return `<table>${body}</table>`;
}

async function copyText(text, html, host) {
  if (navigator.clipboard && window.ClipboardItem) {
    try {
      const item = new ClipboardItem({
        "text/plain": new Blob([text], { type: "text/plain" }),
        "text/html": new Blob([html], { type: "text/html" })
      });
      await navigator.clipboard.write([item]);
      return true;
    } catch (_err) {
      /* iframe: Clipboard API blocked by Permissions-Policy */
    }
  }
  if (navigator.clipboard && navigator.clipboard.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch (_err) {
      /* fall through */
    }
  }
  try {
    const area = document.createElement("textarea");
    area.value = text;
    area.setAttribute("readonly", "");
    area.style.position = "fixed";
    area.style.left = "-9999px";
    document.body.append(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    if (ok) return true;
  } catch (_err) {
    /* fall through */
  }
  if (host && typeof host.copyText === "function") {
    try {
      await host.copyText(text, html);
      return true;
    } catch (_err) {
      /* parent clipboard also unavailable */
    }
  }
  return false;
}

function metricClass(key, row) {
  if (key === "gross_margin") {
    const v = Number(row.gross_margin) || 0;
    if ((Number(row.revenue_rub) || 0) <= 0 && (Number(row.return_sum) || 0) <= 0) return "";
    if (v < 20) return "metric-bad";
    if (v >= 40) return "metric-good";
    return "metric-warn";
  }
  if (key === "drr") {
    const v = Number(row.drr) || 0;
    if ((Number(row.ad_spend) || 0) <= 0) return "";
    if (v > 15) return "metric-bad";
    if (v <= 8) return "metric-good";
    return "metric-warn";
  }
  return "";
}

export async function mount(root, host) {
  root.innerHTML = `
    <main class="kpi">
      <section class="filters">
        <label class="field">Месяц
          <input id="f-month" type="month">
        </label>
        <label class="field">Кабинет
          <select id="f-conn"><option value="">Все кабинеты WB</option></select>
        </label>
        <label class="field">План выручки, ₽
          <input id="f-plan" type="number" min="0" step="1000" placeholder="на месяц">
        </label>
        <div class="kpi-actions">
          <button id="f-refresh" class="btn" type="button">Обновить</button>
          <button id="f-copy" class="btn btn--ghost" type="button" disabled>Копировать в Excel</button>
          <button id="f-csv" class="btn btn--ghost" type="button" disabled>Сохранить CSV</button>
        </div>
      </section>
      <div id="f-status" class="status">Загрузка…</div>
      <div class="table-wrap kpi-scroll">
        <table class="data-table kpi-table">
          <thead id="f-head"></thead>
          <tbody id="f-body"></tbody>
          <tfoot id="f-foot"></tfoot>
        </table>
      </div>
      <p class="kpi-note">Выкупы и выручка — продажи WB на дату события. Заказы — на дату заказа. FBO — склад WB, FBS — склад продавца, DBS/DBW — сумма DBS (доставка продавцом) и DBW. Показы, клики и расход — платная реклама. CR в корзину и CR корз-заказ — воронка (переходы → корзина → заказ), это не те же заказы, что колонка «Заказы шт». ДРР = расход / выручка, CPS = расход / выкупы. Прогноз: прошедшие дни = факт, будущие = средний факт с начала месяца.</p>
    </main>`;

  const monthEl = root.querySelector("#f-month");
  const connEl = root.querySelector("#f-conn");
  const planEl = root.querySelector("#f-plan");
  const refreshEl = root.querySelector("#f-refresh");
  const copyEl = root.querySelector("#f-copy");
  const csvEl = root.querySelector("#f-csv");
  const statusEl = root.querySelector("#f-status");
  const headEl = root.querySelector("#f-head");
  const bodyEl = root.querySelector("#f-body");
  const footEl = root.querySelector("#f-foot");

  let lastPayload = null;
  let copyTimer = null;

  function renderHead() {
    const gRow = document.createElement("tr");
    const dateTh = document.createElement("th");
    dateTh.textContent = "Дата";
    dateTh.rowSpan = 2;
    dateTh.className = "kpi-sticky";
    gRow.append(dateTh);
    for (const group of GROUPS) {
      const th = document.createElement("th");
      th.textContent = group.title;
      th.colSpan = group.cols.length;
      th.className = `kpi-group kpi-group--${group.id}`;
      gRow.append(th);
    }
    const cRow = document.createElement("tr");
    for (const group of GROUPS) {
      for (const col of group.cols) {
        const th = document.createElement("th");
        th.textContent = col.label;
        th.className = `num kpi-col kpi-group--${group.id}`;
        cRow.append(th);
      }
    }
    headEl.replaceChildren(gRow, cRow);
  }

  function appendMetricCells(tr, row, extraClass) {
    for (const group of GROUPS) {
      for (const col of group.cols) {
        const td = document.createElement("td");
        const klass = ["num", extraClass, metricClass(col.key, row)].filter(Boolean).join(" ");
        td.className = klass;
        td.textContent = fmt(col.type, row[col.key]);
        tr.append(td);
      }
    }
  }

  function renderRows(payload) {
    bodyEl.replaceChildren();
    for (const row of payload.rows || []) {
      const tr = document.createElement("tr");
      const dateTd = document.createElement("td");
      dateTd.className = "kpi-sticky";
      dateTd.textContent = humanDate(row.day);
      tr.append(dateTd);
      appendMetricCells(tr, row, "");
      bodyEl.append(tr);
    }
    footEl.replaceChildren();
    if (payload.totals) {
      const tr = document.createElement("tr");
      tr.className = "total";
      const dateTd = document.createElement("td");
      dateTd.className = "kpi-sticky";
      dateTd.textContent = "ИТОГО";
      tr.append(dateTd);
      appendMetricCells(tr, payload.totals, "total");
      footEl.append(tr);
    }
  }

  async function copyExcel() {
    if (!lastPayload || !(lastPayload.rows || []).length) return;
    copyEl.disabled = true;
    try {
      const ok = await copyText(buildExcelTsv(lastPayload), buildExcelHtml(lastPayload), host);
      if (ok) {
        copyEl.textContent = "Скопировано";
        clearTimeout(copyTimer);
        copyTimer = setTimeout(() => {
          copyEl.textContent = "Копировать в Excel";
          copyEl.disabled = false;
        }, 1600);
        return;
      }
      downloadCsv(csvFilename(monthEl.value, "WB KPI"), buildCsv(lastPayload));
      copyEl.disabled = false;
      statusEl.className = "status";
      statusEl.textContent = "Буфер обмена недоступен — таблица сохранена в CSV";
    } catch (err) {
      copyEl.disabled = false;
      statusEl.className = "status status--error";
      statusEl.textContent = err instanceof Error ? err.message : String(err);
    }
  }

  function saveCsv() {
    if (!lastPayload || !(lastPayload.rows || []).length) return;
    downloadCsv(csvFilename(monthEl.value, "WB KPI"), buildCsv(lastPayload));
    csvEl.textContent = "Сохранено";
    setTimeout(() => { csvEl.textContent = "Сохранить CSV"; }, 1600);
  }

  async function load() {
    refreshEl.disabled = true;
    statusEl.className = "status";
    statusEl.textContent = "Загрузка…";
    try {
      const period = monthPeriod(monthEl.value);
      const payload = await host.invoke("loadTable", {
        dateFrom: period.dateFrom,
        dateTo: period.dateTo,
        connectionId: connEl.value,
        monthPlan: planEl.value,
        today: todayIso()
      });
      lastPayload = payload;
      renderRows(payload);
      copyEl.disabled = !(payload.rows || []).length;
      csvEl.disabled = copyEl.disabled;
      const failed = Object.entries(payload.checks || {}).filter(([, ok]) => !ok).map(([k]) => k);
      if (failed.length) {
        statusEl.className = "status status--error";
        statusEl.textContent = `Дней: ${(payload.rows || []).length} · проверка не сошлась: ${failed.join(", ")}`;
      } else {
        statusEl.className = "status status--ok";
        statusEl.textContent = `Дней: ${(payload.rows || []).length} · ${humanDate(payload.dateFrom)} — ${humanDate(payload.dateTo)} · FBO+FBS+DBS/DBW сходятся`;
      }
    } catch (err) {
      lastPayload = null;
      copyEl.disabled = true;
      csvEl.disabled = true;
      bodyEl.replaceChildren();
      footEl.replaceChildren();
      statusEl.className = "status status--error";
      statusEl.textContent = err instanceof Error ? err.message : String(err);
    } finally {
      refreshEl.disabled = false;
    }
  }

  async function loadConnections() {
    try {
      const res = await host.invoke("loadCabinets", {});
      for (const row of res.rows || []) {
        const opt = document.createElement("option");
        opt.value = row.id;
        opt.textContent = row.name || row.id;
        connEl.append(opt);
      }
      monthEl.value = res.suggested_month || currentMonth();
    } catch (err) {
      monthEl.value = currentMonth();
      if (host.log && host.log.warn) host.log.warn("loadCabinets failed", String(err));
    }
  }

  renderHead();
  refreshEl.addEventListener("click", load);
  copyEl.addEventListener("click", copyExcel);
  csvEl.addEventListener("click", saveCsv);
  monthEl.addEventListener("change", load);
  connEl.addEventListener("change", load);
  planEl.addEventListener("change", load);

  await loadConnections();
  await load();
}
