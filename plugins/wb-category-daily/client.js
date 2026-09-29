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

function fmtPct(value) {
  return `${formatNumber(value, 2)}%`;
}

function excelCell(value) {
  const parsed = Number(value);
  const safe = Number.isFinite(parsed) ? parsed : 0;
  return safe.toFixed(2).replace(".", ",");
}

function excelText(value) {
  let text = value == null ? "" : String(value);
  if (/^[=+\-@]/.test(text)) text = `'${text}`;
  return text.replace(/\t/g, " ").replace(/\r?\n/g, " ");
}

function excelRows(payload) {
  const headers = ["Категория", "Показатель"].concat(
    (payload.days || []).map((day) => String(Number(day.slice(8, 10)))),
    ["Итого"]
  );
  const rows = [headers];
  for (const row of payload.rows || []) {
    rows.push(
      [excelText(row.category), excelText(row.label)]
        .concat((row.values || []).map(excelCell), [excelCell(row.total)])
    );
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
    const tag = index === 0 ? "th" : "td";
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
        </table>
      </div>
      <p class="kpi-note">Категория — измерение 1 номенклатуры 1С. VR — видимость в поиске (a040), среднее по товарам, взвешенное переходами из поиска. CTR — клики / показы рекламы WB по товарным строкам a026 (кампании без разбивки по nm_id сюда не входят). CR в корзину и CR корз-заказ — воронка a036 (корзина / переходы и заказы / корзина). Колонка «Итого» считается по суммам месяца, а не как среднее дневных процентов.</p>
    </main>`;

  const monthEl = root.querySelector("#f-month");
  const connEl = root.querySelector("#f-conn");
  const refreshEl = root.querySelector("#f-refresh");
  const copyEl = root.querySelector("#f-copy");
  const csvEl = root.querySelector("#f-csv");
  const statusEl = root.querySelector("#f-status");
  const headEl = root.querySelector("#f-head");
  const bodyEl = root.querySelector("#f-body");

  let lastPayload = null;
  let copyTimer = null;

  function renderHead(days) {
    const tr = document.createElement("tr");
    const cat = document.createElement("th");
    cat.textContent = "Категория";
    cat.className = "kpi-sticky kpi-sticky--cat";
    const metric = document.createElement("th");
    metric.textContent = "Показатель";
    metric.className = "kpi-sticky kpi-sticky--metric";
    tr.append(cat, metric);
    for (const day of days || []) {
      const th = document.createElement("th");
      th.textContent = String(Number(day.slice(8, 10)));
      th.className = "num";
      tr.append(th);
    }
    const total = document.createElement("th");
    total.textContent = "Итого";
    total.className = "num kpi-total";
    tr.append(total);
    headEl.replaceChildren(tr);
  }

  function renderRows(payload) {
    bodyEl.replaceChildren();
    const days = payload.days || [];
    let catIndex = -1;
    let prevCat = null;
    for (const row of payload.rows || []) {
      if (row.category !== prevCat) {
        catIndex += 1;
        prevCat = row.category;
      }
      const tr = document.createElement("tr");
      tr.className = [
        `kpi-metric--${row.metric}`,
        catIndex % 2 === 1 ? "kpi-cat-alt" : ""
      ].filter(Boolean).join(" ");
      const catTd = document.createElement("td");
      catTd.className = "kpi-sticky kpi-sticky--cat";
      catTd.textContent = row.category;
      const metricTd = document.createElement("td");
      metricTd.className = "kpi-sticky kpi-sticky--metric";
      metricTd.textContent = row.label;
      tr.append(catTd, metricTd);
      const values = row.values || [];
      for (let i = 0; i < days.length; i += 1) {
        const td = document.createElement("td");
        td.className = "num";
        td.textContent = fmtPct(values[i]);
        tr.append(td);
      }
      const totalTd = document.createElement("td");
      totalTd.className = "num kpi-total";
      totalTd.textContent = fmtPct(row.total);
      tr.append(totalTd);
      bodyEl.append(tr);
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
      downloadCsv(csvFilename(monthEl.value, "WB категории"), buildCsv(lastPayload));
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
    downloadCsv(csvFilename(monthEl.value, "WB категории"), buildCsv(lastPayload));
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
        connectionId: connEl.value
      });
      lastPayload = payload;
      renderHead(payload.days || []);
      renderRows(payload);
      const cats = new Set((payload.rows || []).map((row) => row.category)).size;
      copyEl.disabled = !(payload.rows || []).length;
      csvEl.disabled = copyEl.disabled;
      statusEl.className = "status status--ok";
      statusEl.textContent = `Категорий: ${cats} · ${humanDate(payload.dateFrom)} — ${humanDate(payload.dateTo)}`;
    } catch (err) {
      lastPayload = null;
      copyEl.disabled = true;
      csvEl.disabled = true;
      headEl.replaceChildren();
      bodyEl.replaceChildren();
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

  refreshEl.addEventListener("click", load);
  copyEl.addEventListener("click", copyExcel);
  csvEl.addEventListener("click", saveCsv);
  monthEl.addEventListener("change", load);
  connEl.addEventListener("change", load);

  await loadConnections();
  await load();
}
