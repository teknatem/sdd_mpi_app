/*!
 * PluginTables — тема-aware рантайм таблиц данных для плагинов (без зависимостей).
 *
 * Грузится в iframe плагина (как plugin-charts.js). Плагин в client_script зовёт:
 *
 *   const rows = await host.invoke("data", {});
 *   PluginTables.render(root, spec, rows);
 *
 * spec (компактная форма, удобная для LLM):
 *   {
 *     title: "Заголовок",
 *     columns: [
 *       { key:"article", label:"Артикул", type:"text", align:"left", width:"180px" },
 *       { key:"revenue", label:"Выручка", type:"money", align:"right", format:"money" }
 *     ],
 *     sort: { key:"revenue", dir:"desc" },
 *     filters: { global:true, perColumn:true },
 *     conditionalFormat: [
 *       { column:"revenue", kind:"dataBar", color:"primary" },
 *       { column:"margin", kind:"threshold", rules:[
 *           { op:"<", value:0, color:"error", target:"text" },
 *           { op:">=", value:0.3, color:"success", target:"bg" } ] },
 *       { column:"qty", kind:"heatmap", min:"error", mid:"warning", max:"success" }
 *     ],
 *     totals: { enabled:true, agg:{ revenue:"sum", qty:"sum", margin:"avg" } },
 *     pagination: { enabled:true, pageSize:50 },
 *     export: { csv:true, clipboard:true }
 *   }
 *
 * Тип колонки (type): text | number | int | money | percent | date.
 * Цвета берутся из CSS-переменных активной темы — таблица совпадает со светлой/тёмной
 * темой приложения. PluginTables.applyTheme() перерисовывает живые таблицы при смене темы
 * (вызывается из bootstrap iframe).
 *
 * Сортировка/фильтрация/формат/итоги/пагинация/экспорт — всё КЛИЕНТ-САЙД, над уже
 * загруженным массивом строк (никаких новых запросов к серверу).
 */
(function () {
  "use strict";

  if (window.PluginTables) return;

  // Контроллеры живых таблиц: { rebuild(), destroy() }.
  var REGISTRY = new Set();
  var STYLE_ID = "ptables-style";

  function injectStyleOnce() {
    if (document.getElementById(STYLE_ID)) return;
    var css =
      ".ptables{display:flex;flex-direction:column;gap:8px;min-height:0;height:100%;" +
      "color:var(--color-text-primary,#e5e7eb);font-size:13px;}" +
      ".ptables__toolbar{display:flex;align-items:center;gap:8px;flex-wrap:wrap;}" +
      ".ptables__title{font-weight:600;font-size:15px;margin-right:auto;}" +
      ".ptables__search{flex:0 1 220px;min-width:120px;padding:4px 9px;border-radius:6px;" +
      "border:1px solid var(--color-border,rgba(128,128,128,.4));background:var(--color-surface,#1f2937);" +
      "color:var(--color-text-primary,#e5e7eb);font-size:13px;}" +
      ".ptables__btn{cursor:pointer;border:1px solid var(--color-border,rgba(128,128,128,.4));" +
      "background:transparent;color:var(--color-text-secondary,#9ca3af);border-radius:6px;" +
      "padding:4px 11px;font-size:12px;line-height:1.4;transition:all .12s;}" +
      ".ptables__btn:hover{color:var(--color-text-primary,#e5e7eb);border-color:var(--color-primary,#4f8cff);}" +
      ".ptables__menu{position:relative;display:inline-block;}" +
      ".ptables__menu-pop{position:absolute;z-index:20;top:calc(100% + 4px);right:0;min-width:180px;" +
      "max-height:260px;overflow:auto;padding:6px;border-radius:8px;background:var(--color-surface,#1f2937);" +
      "border:1px solid var(--color-border,rgba(128,128,128,.4));box-shadow:0 8px 24px rgba(0,0,0,.35);}" +
      ".ptables__menu-item{display:flex;align-items:center;gap:8px;padding:4px 6px;border-radius:5px;" +
      "cursor:pointer;font-size:13px;white-space:nowrap;}" +
      ".ptables__menu-item:hover{background:var(--color-border-light,rgba(128,128,128,.15));}" +
      ".ptables__scroll{flex:1 1 auto;min-height:0;overflow:auto;border:1px solid " +
      "var(--color-border,rgba(128,128,128,.3));border-radius:8px;}" +
      ".ptables__table{border-collapse:separate;border-spacing:0;width:100%;font-variant-numeric:tabular-nums;}" +
      ".ptables__table th,.ptables__table td{padding:6px 10px;border-bottom:1px solid " +
      "var(--color-border-light,rgba(128,128,128,.18));white-space:nowrap;overflow:hidden;text-overflow:ellipsis;}" +
      ".ptables__table thead th{position:sticky;top:0;z-index:2;background:var(--color-surface,#1f2937);" +
      "color:var(--color-text-secondary,#9ca3af);font-weight:600;text-align:left;}" +
      ".ptables__th-sort{cursor:pointer;user-select:none;}" +
      ".ptables__th-sort:hover{color:var(--color-text-primary,#e5e7eb);}" +
      ".ptables__sort-ind{opacity:.5;font-size:10px;margin-left:4px;}" +
      ".ptables__sort-ind--on{opacity:1;color:var(--color-primary,#4f8cff);}" +
      ".ptables__filter-row th{position:sticky;top:var(--ptables-head-h,30px);z-index:1;" +
      "background:var(--color-surface,#1f2937);padding:4px 6px;}" +
      ".ptables__fin{width:100%;box-sizing:border-box;padding:3px 6px;border-radius:5px;" +
      "border:1px solid var(--color-border,rgba(128,128,128,.4));background:var(--color-bg,transparent);" +
      "color:var(--color-text-primary,#e5e7eb);font-size:12px;}" +
      ".ptables__table tbody tr:hover td{background:var(--color-border-light,rgba(128,128,128,.10));}" +
      ".ptables__cell-bar{position:relative;}" +
      ".ptables__cell-bar > span{position:relative;z-index:1;}" +
      ".ptables__table tfoot td{position:sticky;bottom:0;background:var(--color-surface,#1f2937);" +
      "font-weight:600;border-top:2px solid var(--color-border,rgba(128,128,128,.4));}" +
      ".ptables__footer{display:flex;align-items:center;gap:10px;flex-wrap:wrap;" +
      "color:var(--color-text-secondary,#9ca3af);font-size:12px;}" +
      ".ptables__footer .ptables__spacer{margin-left:auto;}" +
      ".ptables__empty{padding:24px;color:var(--color-text-secondary,#9ca3af);text-align:center;font-size:14px;}";
    var el = document.createElement("style");
    el.id = STYLE_ID;
    el.textContent = css;
    document.head.appendChild(el);
  }

  function cssVar(name, fallback) {
    try {
      var v = getComputedStyle(document.documentElement)
        .getPropertyValue(name)
        .trim();
      return v || fallback;
    } catch (e) {
      return fallback;
    }
  }

  // Семантические имена цветов → CSS-переменные тем (для условного форматирования).
  function resolveColor(name) {
    if (!name) return null;
    var map = {
      success: "--color-success",
      warning: "--color-warning",
      error: "--color-error",
      danger: "--color-error",
      primary: "--color-primary",
    };
    if (map[name]) return cssVar(map[name], name);
    return name; // уже hex/rgb/css-цвет
  }

  function withAlpha(color, alpha) {
    var h = String(color).replace("#", "");
    if (h.length === 3) h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
    var n = parseInt(h, 16);
    if (isNaN(n) || h.length < 6) return color; // не hex — отдаём как есть
    var r = (n >> 16) & 255, g = (n >> 8) & 255, b = n & 255;
    return "rgba(" + r + "," + g + "," + b + "," + alpha + ")";
  }

  function num(value) {
    if (value === null || value === undefined || value === "") return null;
    var n = typeof value === "number" ? value : parseFloat(String(value).replace(",", "."));
    return isNaN(n) ? null : n;
  }

  function isNumericType(type) {
    return type === "number" || type === "int" || type === "money" || type === "percent";
  }

  function formatter(col) {
    var type = col.type;
    var fmtName = col.format || type;
    var ru = function (opts) {
      try {
        return new Intl.NumberFormat("ru-RU", opts);
      } catch (e) {
        return { format: function (v) { return String(v); } };
      }
    };
    if (fmtName === "money") {
      var m = ru({ maximumFractionDigits: 0 });
      return function (v) { var n = num(v); return n === null ? "" : m.format(n) + " ₽"; };
    }
    if (fmtName === "percent") {
      var p = ru({ maximumFractionDigits: 1 });
      return function (v) {
        var n = num(v);
        if (n === null) return "";
        // Percent values use the analytics-friendly fractional convention:
        // 0.34 is displayed as 34%, while already-scaled values like 34 stay 34%.
        var shown = Math.abs(n) <= 1 ? n * 100 : n;
        return p.format(shown) + "%";
      };
    }
    if (fmtName === "int") {
      var i = ru({ maximumFractionDigits: 0 });
      return function (v) { var n = num(v); return n === null ? "" : i.format(n); };
    }
    if (fmtName === "number") {
      var d = ru({ maximumFractionDigits: 2 });
      return function (v) { var n = num(v); return n === null ? "" : d.format(n); };
    }
    // text | date — как есть
    return function (v) { return v === null || v === undefined ? "" : String(v); };
  }

  // ── Конвейер данных (клиент-сайд) ──────────────────────────────────────────
  function applyGlobalSearch(rows, query, columns) {
    var q = String(query || "").trim().toLowerCase();
    if (!q) return rows;
    return rows.filter(function (r) {
      for (var i = 0; i < columns.length; i++) {
        var v = r[columns[i].key];
        if (v !== null && v !== undefined && String(v).toLowerCase().indexOf(q) !== -1) return true;
      }
      return false;
    });
  }

  // Числовой предикат из строки фильтра: ">100", "<=5", "10..20", "=3" или просто "100".
  function numericPredicate(expr) {
    var s = String(expr).trim().replace(/\s+/g, "");
    if (!s) return null;
    var range = s.match(/^(-?[\d.,]+)\.\.(-?[\d.,]+)$/);
    if (range) {
      var lo = num(range[1]), hi = num(range[2]);
      return function (n) { return n !== null && n >= lo && n <= hi; };
    }
    var m = s.match(/^(>=|<=|>|<|=)?(-?[\d.,]+)$/);
    if (!m) return null;
    if (!m[1]) {
      // без оператора — "содержит" по строковому представлению
      return function (n, raw) { return String(raw).indexOf(m[2]) !== -1; };
    }
    var val = num(m[2]);
    if (val === null) return null;
    switch (m[1]) {
      case ">": return function (n) { return n !== null && n > val; };
      case "<": return function (n) { return n !== null && n < val; };
      case ">=": return function (n) { return n !== null && n >= val; };
      case "<=": return function (n) { return n !== null && n <= val; };
      case "=": return function (n) { return n !== null && n === val; };
      default: return null;
    }
  }

  function filterNumber(col, raw) {
    var n = num(raw);
    if (n === null) return null;
    return col.type === "percent" && Math.abs(n) <= 1 ? n * 100 : n;
  }

  function applyColumnFilters(rows, filters, columns) {
    var active = [];
    columns.forEach(function (c) {
      var f = filters[c.key];
      if (f === undefined || f === null || String(f).trim() === "") return;
      if (isNumericType(c.type)) {
        var pred = numericPredicate(f);
        if (pred) active.push({ key: c.key, col: c, test: function (raw, col) { return pred(filterNumber(col, raw), raw); } });
      } else {
        var q = String(f).trim().toLowerCase();
        active.push({ key: c.key, test: function (raw) {
          return raw !== null && raw !== undefined && String(raw).toLowerCase().indexOf(q) !== -1;
        } });
      }
    });
    if (!active.length) return rows;
    return rows.filter(function (r) {
      for (var i = 0; i < active.length; i++) {
        if (!active[i].test(r[active[i].key], active[i].col)) return false;
      }
      return true;
    });
  }

  function validateSpec(spec, rows) {
    var errors = [];
    var allowedTypes = { text: 1, number: 1, int: 1, money: 1, percent: 1, date: 1 };
    var allowedAgg = { sum: 1, avg: 1, count: 1, min: 1, max: 1 };
    var allowedCf = { threshold: 1, dataBar: 1, heatmap: 1 };
    var cols = Array.isArray(spec && spec.columns) ? spec.columns : [];
    var keys = {};
    if (!cols.length) errors.push("spec.columns must contain at least one column");
    cols.forEach(function (c, i) {
      if (!c || !c.key) errors.push("columns[" + i + "].key is required");
      if (c && c.key && keys[c.key]) errors.push("duplicate column key: " + c.key);
      if (c && c.key) keys[c.key] = 1;
      if (c && c.type && !allowedTypes[c.type]) errors.push("unsupported column type for " + c.key + ": " + c.type);
    });
    if (spec && spec.sort && spec.sort.key && !keys[spec.sort.key]) errors.push("sort.key is not in columns: " + spec.sort.key);
    if (spec && spec.totals && spec.totals.agg) {
      Object.keys(spec.totals.agg).forEach(function (key) {
        if (!keys[key]) errors.push("totals column is not in columns: " + key);
        if (!allowedAgg[spec.totals.agg[key]]) errors.push("unsupported totals agg for " + key + ": " + spec.totals.agg[key]);
      });
    }
    (spec && spec.conditionalFormat || []).forEach(function (cf, i) {
      if (!cf || !keys[cf.column]) errors.push("conditionalFormat[" + i + "].column is not in columns: " + (cf && cf.column));
      if (!cf || !allowedCf[cf.kind]) errors.push("unsupported conditionalFormat kind: " + (cf && cf.kind));
    });
    var sample = Array.isArray(rows) && rows.length && rows[0] && typeof rows[0] === "object" ? rows[0] : null;
    if (sample) {
      cols.forEach(function (c) {
        if (c && c.key && !(c.key in sample)) errors.push("column key is absent in data rows: " + c.key);
      });
    }
    return { ok: errors.length === 0, errors: errors };
  }

  function applySort(rows, sortKey, sortDir, columns) {
    if (!sortKey || !sortDir) return rows;
    var col = columns.filter(function (c) { return c.key === sortKey; })[0];
    var numeric = col ? isNumericType(col.type) : false;
    var isDate = col && col.type === "date";
    var sign = sortDir === "desc" ? -1 : 1;
    var copy = rows.slice();
    copy.sort(function (a, b) {
      var av = a[sortKey], bv = b[sortKey];
      var cmp;
      if (numeric) {
        var an = num(av), bn = num(bv);
        if (an === null && bn === null) cmp = 0;
        else if (an === null) cmp = 1;       // пустые — в конец
        else if (bn === null) cmp = -1;
        else cmp = an - bn;
      } else if (isDate) {
        var ad = Date.parse(av), bd = Date.parse(bv);
        if (isNaN(ad) && isNaN(bd)) cmp = String(av).localeCompare(String(bv), "ru");
        else if (isNaN(ad)) cmp = 1;
        else if (isNaN(bd)) cmp = -1;
        else cmp = ad - bd;
      } else {
        cmp = String(av === null || av === undefined ? "" : av)
          .localeCompare(String(bv === null || bv === undefined ? "" : bv), "ru");
      }
      return cmp * sign;
    });
    return copy;
  }

  function aggregate(rows, key, fn) {
    var nums = [];
    for (var i = 0; i < rows.length; i++) {
      var n = num(rows[i][key]);
      if (n !== null) nums.push(n);
    }
    if (fn === "count") return rows.length;
    if (!nums.length) return null;
    if (fn === "sum") return nums.reduce(function (a, b) { return a + b; }, 0);
    if (fn === "avg") return nums.reduce(function (a, b) { return a + b; }, 0) / nums.length;
    if (fn === "min") return Math.min.apply(null, nums);
    if (fn === "max") return Math.max.apply(null, nums);
    return null;
  }

  function colStats(rows, key) {
    var min = Infinity, max = -Infinity, has = false;
    for (var i = 0; i < rows.length; i++) {
      var n = num(rows[i][key]);
      if (n === null) continue;
      has = true;
      if (n < min) min = n;
      if (n > max) max = n;
    }
    return has ? { min: min, max: max } : null;
  }

  // ── Условное форматирование одной ячейки ───────────────────────────────────
  function buildFormatIndex(specCF) {
    var byCol = {};
    (specCF || []).forEach(function (cf) {
      if (!cf || !cf.column) return;
      (byCol[cf.column] = byCol[cf.column] || []).push(cf);
    });
    return byCol;
  }

  function lerpColor(c1, c2, t) {
    function parse(c) {
      var h = String(c).replace("#", "");
      if (h.length === 3) h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
      var n = parseInt(h, 16);
      if (isNaN(n)) return null;
      return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    }
    var a = parse(c1), b = parse(c2);
    if (!a || !b) return c2;
    var r = Math.round(a[0] + (b[0] - a[0]) * t);
    var g = Math.round(a[1] + (b[1] - a[1]) * t);
    var bl = Math.round(a[2] + (b[2] - a[2]) * t);
    return "rgb(" + r + "," + g + "," + bl + ")";
  }

  // Применяет правила форматирования к <td> (и вкладывает <span> для dataBar).
  function applyCellFormat(td, span, rawValue, cfList, stats) {
    var n = num(rawValue);
    cfList.forEach(function (cf) {
      if (cf.kind === "threshold") {
        (cf.rules || []).forEach(function (rule) {
          if (n === null) return;
          var v = num(rule.value);
          var hit = false;
          switch (rule.op) {
            case ">": hit = n > v; break;
            case "<": hit = n < v; break;
            case ">=": hit = n >= v; break;
            case "<=": hit = n <= v; break;
            case "=": hit = n === v; break;
            case "!=": hit = n !== v; break;
          }
          if (!hit) return;
          var color = resolveColor(rule.color) || cssVar("--color-primary", "#4f8cff");
          if (rule.target === "bg") {
            td.style.background = withAlpha(color, rule.alpha || 0.18);
            td.style.color = color;
          } else {
            td.style.color = color;
            td.style.fontWeight = "600";
          }
        });
      } else if (cf.kind === "dataBar") {
        if (n === null || !stats || stats.max <= 0) return;
        var base = resolveColor(cf.color) || cssVar("--color-primary", "#4f8cff");
        var denom = stats.max > 0 ? stats.max : 1;
        var pct = Math.max(0, Math.min(100, (n / denom) * 100));
        td.classList.add("ptables__cell-bar");
        td.style.backgroundImage =
          "linear-gradient(90deg," + withAlpha(base, 0.32) + " " + pct + "%, transparent " + pct + "%)";
      } else if (cf.kind === "heatmap") {
        if (n === null || !stats || stats.max === stats.min) return;
        var lo = resolveColor(cf.min || "error") || "#ef4444";
        var mid = resolveColor(cf.mid || "warning") || "#f59e0b";
        var hi = resolveColor(cf.max || "success") || "#22c55e";
        var t = (n - stats.min) / (stats.max - stats.min);
        var col = t < 0.5 ? lerpColor(lo, mid, t * 2) : lerpColor(mid, hi, (t - 0.5) * 2);
        td.style.background = withAlpha(col, 0.55);
      }
    });
  }

  // ── CSV / clipboard ────────────────────────────────────────────────────────
  function visibleColumns(columns, hidden) {
    return columns.filter(function (c) { return !c.hidden && !hidden.has(c.key); });
  }

  function csvCell(v) {
    var s = v === null || v === undefined ? "" : String(v);
    if (/[",\n\r]/.test(s)) return '"' + s.replace(/"/g, '""') + '"';
    return s;
  }

  function exportCsv(title, columns, rows) {
    var lines = [columns.map(function (c) { return csvCell(c.label || c.key); }).join(",")];
    rows.forEach(function (r) {
      lines.push(columns.map(function (c) { return csvCell(r[c.key]); }).join(","));
    });
    var blob = new Blob(["﻿" + lines.join("\r\n")], { type: "text/csv;charset=utf-8;" });
    var url = URL.createObjectURL(blob);
    var a = document.createElement("a");
    a.href = url;
    a.download = (title || "table").replace(/[^\w\-а-яё ]+/gi, "").trim().slice(0, 60) + ".csv";
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(function () { URL.revokeObjectURL(url); }, 1000);
  }

  function copyTsv(columns, rows) {
    var lines = [columns.map(function (c) { return c.label || c.key; }).join("\t")];
    rows.forEach(function (r) {
      lines.push(columns.map(function (c) {
        var v = r[c.key];
        return (v === null || v === undefined ? "" : String(v)).replace(/[\t\n\r]+/g, " ");
      }).join("\t"));
    });
    return copyPlain(lines.join("\n"));
  }

  function copyPlain(text) {
    return new Promise(function (resolve) {
      if (navigator.clipboard && navigator.clipboard.writeText) {
        try {
          navigator.clipboard.writeText(text).then(function () { resolve(true); }, function () {
            fallbackCopy(text).then(resolve);
          });
          return;
        } catch (e) {
          /* Permissions-Policy throws synchronously */
        }
      }
      fallbackCopy(text).then(resolve);
    });
  }

  function fallbackCopy(text) {
    return new Promise(function (resolve) {
      try {
        var ta = document.createElement("textarea");
        ta.value = text;
        ta.style.position = "fixed";
        ta.style.opacity = "0";
        document.body.appendChild(ta);
        ta.select();
        var ok = document.execCommand("copy");
        document.body.removeChild(ta);
        if (ok) {
          resolve(true);
          return;
        }
      } catch (e) { /* буфер недоступен в sandbox */ }
      var host = window.__pluginHost;
      if (host && typeof host.copyText === "function") {
        Promise.resolve(host.copyText(text)).then(function () { resolve(true); }, function () { resolve(false); });
        return;
      }
      resolve(false);
    });
  }

  // ── Публичный render ───────────────────────────────────────────────────────
  function render(target, spec, rows) {
    injectStyleOnce();
    spec = spec || {};
    var columns = (spec.columns || []).map(function (c) {
      return {
        key: c.key,
        label: c.label || c.key,
        type: c.type || "text",
        align: c.align || (isNumericType(c.type) ? "right" : "left"),
        width: c.width || null,
        format: c.format || null,
        hidden: !!c.hidden,
      };
    });
    var allRows = Array.isArray(rows) ? rows : [];
    // Диагностика для автора/LLM: ключ колонки, которого нет в данных, рисует пустой столбец
    // молча — подсветим в консоли (видно в smoke-test/devtools), чтобы поймать рассинхрон с SELECT.
    if (allRows.length) {
      var sample = allRows[0];
      columns.forEach(function (c) {
        if (!(c.key in sample)) {
          try {
            console.warn("[PluginTables] колонка '" + c.key + "' отсутствует в данных — проверь алиас в SELECT");
          } catch (e) { /* noop */ }
        }
      });
    }
    var filtersCfg = spec.filters || {};
    var totalsCfg = spec.totals || {};
    var pagingCfg = spec.pagination || {};
    var exportCfg = spec.export || {};
    var cfIndex = buildFormatIndex(spec.conditionalFormat);

    var root = target;
    root.classList.add("ptables");
    root.replaceChildren();

    // Состояние представления.
    var state = {
      sortKey: spec.sort && spec.sort.key ? spec.sort.key : null,
      sortDir: spec.sort && spec.sort.dir ? spec.sort.dir : null,
      filters: {},
      globalQuery: "",
      page: 0,
      pageSize: pagingCfg.enabled === false ? 0 : (pagingCfg.pageSize || 50),
      hidden: new Set(),
    };
    columns.forEach(function (c) { if (c.hidden) state.hidden.add(c.key); });

    // ── Тулбар ──
    var toolbar = document.createElement("div");
    toolbar.className = "ptables__toolbar";
    var title = document.createElement("div");
    title.className = "ptables__title";
    title.textContent = spec.title || "";
    toolbar.appendChild(title);

    if (filtersCfg.global !== false) {
      var search = document.createElement("input");
      search.className = "ptables__search";
      search.type = "search";
      search.placeholder = "Поиск…";
      var deb = null;
      search.addEventListener("input", function () {
        clearTimeout(deb);
        deb = setTimeout(function () {
          state.globalQuery = search.value;
          state.page = 0;
          invalidate();
          renderBody();
        }, 180);
      });
      toolbar.appendChild(search);
    }

    // Меню колонок.
    var colMenu = document.createElement("div");
    colMenu.className = "ptables__menu";
    var colBtn = document.createElement("button");
    colBtn.className = "ptables__btn";
    colBtn.type = "button";
    colBtn.textContent = "Колонки";
    var colPop = document.createElement("div");
    colPop.className = "ptables__menu-pop";
    colPop.style.display = "none";
    columns.forEach(function (c) {
      var item = document.createElement("label");
      item.className = "ptables__menu-item";
      var cb = document.createElement("input");
      cb.type = "checkbox";
      cb.checked = !state.hidden.has(c.key);
      cb.addEventListener("change", function () {
        if (cb.checked) state.hidden.delete(c.key); else state.hidden.add(c.key);
        invalidate(); // видимость колонок влияет на глобальный поиск
        renderHead();
        renderBody();
      });
      item.appendChild(cb);
      item.appendChild(document.createTextNode(c.label));
      colPop.appendChild(item);
    });
    colBtn.addEventListener("click", function () {
      colPop.style.display = colPop.style.display === "none" ? "block" : "none";
    });
    // Закрытие меню по клику вне него. Слушатель снимается в destroy()/unmount(),
    // иначе при рестарте плагина в том же iframe обработчики копятся.
    var onDocClick = function (e) {
      if (!colMenu.contains(e.target)) colPop.style.display = "none";
    };
    document.addEventListener("click", onDocClick);
    colMenu.appendChild(colBtn);
    colMenu.appendChild(colPop);
    toolbar.appendChild(colMenu);

    if (exportCfg.csv !== false) {
      var csvBtn = document.createElement("button");
      csvBtn.className = "ptables__btn";
      csvBtn.type = "button";
      csvBtn.textContent = "CSV";
      csvBtn.addEventListener("click", function () {
        exportCsv(spec.title, visibleColumns(columns, state.hidden), currentFiltered());
      });
      toolbar.appendChild(csvBtn);
    }
    if (exportCfg.clipboard !== false) {
      var copyBtn = document.createElement("button");
      copyBtn.className = "ptables__btn";
      copyBtn.type = "button";
      copyBtn.textContent = "Копировать";
      copyBtn.addEventListener("click", function () {
        var was = copyBtn.textContent;
        copyTsv(visibleColumns(columns, state.hidden), currentFiltered()).then(function (ok) {
          if (!ok) {
            exportCsv(spec.title, visibleColumns(columns, state.hidden), currentFiltered());
            copyBtn.textContent = "CSV";
            setTimeout(function () { copyBtn.textContent = was; }, 1600);
            return;
          }
          copyBtn.textContent = "Скопировано";
          setTimeout(function () { copyBtn.textContent = was; }, 1200);
        });
      });
      toolbar.appendChild(copyBtn);
    }
    root.appendChild(toolbar);

    // ── Таблица ──
    var scroll = document.createElement("div");
    scroll.className = "ptables__scroll";
    var table = document.createElement("table");
    table.className = "ptables__table";
    var thead = document.createElement("thead");
    var tbody = document.createElement("tbody");
    var tfoot = document.createElement("tfoot");
    table.appendChild(thead);
    table.appendChild(tbody);
    table.appendChild(tfoot);
    scroll.appendChild(table);
    root.appendChild(scroll);

    // ── Подвал (пагинация/счётчик) ──
    var footer = document.createElement("div");
    footer.className = "ptables__footer";
    root.appendChild(footer);

    // Производные выборки. Конвейер (поиск→фильтры→сортировка→colStats) считается один раз
    // и кэшируется; пагинация и rebuild() (смена темы) переиспользуют результат, не пересортировывая.
    // Кэш сбрасывается через invalidate() при смене поиска/фильтров/сортировки/видимости колонок.
    var pipelineCache = null;

    function computePipeline() {
      var vis = visibleColumns(columns, state.hidden);
      var r = applyGlobalSearch(allRows, state.globalQuery, vis);
      r = applyColumnFilters(r, state.filters, columns);
      r = applySort(r, state.sortKey, state.sortDir, columns);
      var stats = {};
      vis.forEach(function (c) {
        if (cfIndex[c.key]) {
          var needs = cfIndex[c.key].some(function (cf) { return cf.kind === "dataBar" || cf.kind === "heatmap"; });
          if (needs) stats[c.key] = colStats(r, c.key);
        }
      });
      return { rows: r, stats: stats };
    }

    function pipeline() {
      if (!pipelineCache) pipelineCache = computePipeline();
      return pipelineCache;
    }

    function invalidate() { pipelineCache = null; }

    function currentFiltered() { return pipeline().rows; }

    function renderHead() {
      thead.replaceChildren();
      var vis = visibleColumns(columns, state.hidden);
      var hr = document.createElement("tr");
      vis.forEach(function (c) {
        var th = document.createElement("th");
        th.className = "ptables__th-sort";
        if (c.width) th.style.width = c.width;
        th.style.textAlign = c.align;
        var label = document.createElement("span");
        label.textContent = c.label;
        th.appendChild(label);
        var ind = document.createElement("span");
        ind.className = "ptables__sort-ind";
        if (state.sortKey === c.key && state.sortDir) {
          ind.className += " ptables__sort-ind--on";
          ind.textContent = state.sortDir === "asc" ? "▲" : "▼";
        } else {
          ind.textContent = "⇅";
        }
        th.appendChild(ind);
        th.addEventListener("click", function () {
          if (state.sortKey !== c.key) { state.sortKey = c.key; state.sortDir = "asc"; }
          else if (state.sortDir === "asc") state.sortDir = "desc";
          else if (state.sortDir === "desc") { state.sortKey = null; state.sortDir = null; }
          else state.sortDir = "asc";
          state.page = 0;
          invalidate();
          renderHead();
          renderBody();
        });
        hr.appendChild(th);
      });
      thead.appendChild(hr);

      if (filtersCfg.perColumn) {
        var fr = document.createElement("tr");
        fr.className = "ptables__filter-row";
        vis.forEach(function (c) {
          var th = document.createElement("th");
          var inp = document.createElement("input");
          inp.className = "ptables__fin";
          inp.type = "text";
          inp.value = state.filters[c.key] || "";
          inp.placeholder = isNumericType(c.type) ? ">0  <100  1..5" : "содержит…";
          var d = null;
          inp.addEventListener("input", function () {
            clearTimeout(d);
            d = setTimeout(function () {
              state.filters[c.key] = inp.value;
              state.page = 0;
              invalidate();
              renderBody();
            }, 180);
          });
          // не сортировать при клике по строке фильтров
          th.addEventListener("click", function (e) { e.stopPropagation(); });
          th.appendChild(inp);
          fr.appendChild(th);
        });
        thead.appendChild(fr);
        // Строка фильтров «прилипает» под строкой заголовков — её sticky-top равен фактической
        // высоте шапки (а не хардкоду). Меряем после раскладки.
        requestAnimationFrame(function () {
          var h = hr.offsetHeight;
          if (h) root.style.setProperty("--ptables-head-h", h + "px");
        });
      }
    }

    function renderBody() {
      var vis = visibleColumns(columns, state.hidden);
      var p = pipeline();
      var filtered = p.rows;
      var statsCache = p.stats;

      var total = filtered.length;
      var pageSize = state.pageSize;
      var pageRows = filtered;
      var pageCount = 1;
      if (pageSize && total > pageSize) {
        pageCount = Math.ceil(total / pageSize);
        if (state.page >= pageCount) state.page = pageCount - 1;
        var start = state.page * pageSize;
        pageRows = filtered.slice(start, start + pageSize);
      }

      tbody.replaceChildren();
      if (!pageRows.length) {
        var tr = document.createElement("tr");
        var td = document.createElement("td");
        td.colSpan = vis.length || 1;
        td.className = "ptables__empty";
        td.textContent = allRows.length ? "Ничего не найдено по фильтрам" : "Нет данных";
        tr.appendChild(td);
        tbody.appendChild(tr);
      } else {
        pageRows.forEach(function (r) {
          var tr = document.createElement("tr");
          vis.forEach(function (c) {
            var td = document.createElement("td");
            td.style.textAlign = c.align;
            var fmt = formatter(c);
            var span = document.createElement("span");
            span.textContent = fmt(r[c.key]);
            td.appendChild(span);
            if (cfIndex[c.key]) applyCellFormat(td, span, r[c.key], cfIndex[c.key], statsCache[c.key]);
            tr.appendChild(td);
          });
          tbody.appendChild(tr);
        });
      }

      // Итоги.
      tfoot.replaceChildren();
      if (totalsCfg.enabled && totalsCfg.agg) {
        var tr2 = document.createElement("tr");
        vis.forEach(function (c, idx) {
          var td = document.createElement("td");
          td.style.textAlign = c.align;
          var fn = totalsCfg.agg[c.key];
          if (fn) {
            var val = aggregate(filtered, c.key, fn);
            var label = fn === "count" ? "" : "";
            if (val !== null) td.textContent = label + formatter(c)(val);
          } else if (idx === 0) {
            td.textContent = "Итого";
            td.style.color = "var(--color-text-secondary,#9ca3af)";
          }
          tr2.appendChild(td);
        });
        tfoot.appendChild(tr2);
      }

      // Подвал.
      footer.replaceChildren();
      var count = document.createElement("span");
      var shownFrom = pageSize && total > pageSize ? state.page * pageSize + 1 : (total ? 1 : 0);
      var shownTo = pageSize && total > pageSize ? Math.min(total, (state.page + 1) * pageSize) : total;
      count.textContent = total === allRows.length
        ? "Строк: " + total
        : shownFrom + "–" + shownTo + " из " + total + " (всего " + allRows.length + ")";
      footer.appendChild(count);

      if (pageSize && pageCount > 1) {
        var spacer = document.createElement("span");
        spacer.className = "ptables__spacer";
        footer.appendChild(spacer);

        var prev = document.createElement("button");
        prev.className = "ptables__btn";
        prev.type = "button";
        prev.textContent = "‹";
        prev.disabled = state.page === 0;
        prev.addEventListener("click", function () { if (state.page > 0) { state.page--; renderBody(); } });
        footer.appendChild(prev);

        var pg = document.createElement("span");
        pg.textContent = (state.page + 1) + " / " + pageCount;
        footer.appendChild(pg);

        var next = document.createElement("button");
        next.className = "ptables__btn";
        next.type = "button";
        next.textContent = "›";
        next.disabled = state.page >= pageCount - 1;
        next.addEventListener("click", function () { if (state.page < pageCount - 1) { state.page++; renderBody(); } });
        footer.appendChild(next);
      }
    }

    if (!columns.length) {
      scroll.replaceChildren();
      var warn = document.createElement("div");
      warn.className = "ptables__empty";
      warn.textContent = "Не заданы колонки (spec.columns)";
      scroll.appendChild(warn);
    } else {
      renderHead();
      renderBody();
    }

    var controller = {
      rebuild: function () {
        // Тема сменилась — пересчитать вычисляемые цвета (heatmap/dataBar/threshold).
        if (columns.length) { renderHead(); renderBody(); }
      },
      destroy: function () {
        document.removeEventListener("click", onDocClick);
        REGISTRY.delete(controller);
      },
    };
    REGISTRY.add(controller);
    return controller;
  }

  // Перерисовать все живые таблицы (смена темы приложения).
  function applyTheme() {
    REGISTRY.forEach(function (c) {
      try { c.rebuild(); } catch (e) { /* таблица могла быть размонтирована */ }
    });
  }

  window.PluginTables = {
    render: render,
    validateSpec: validateSpec,
    applyTheme: applyTheme,
  };
})();
