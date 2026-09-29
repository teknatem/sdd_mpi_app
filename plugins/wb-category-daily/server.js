function text(value) {
  return value == null ? "" : String(value);
}

function num(value) {
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function cabinet(args) {
  return args && args.connectionId ? String(args.connectionId) : "";
}

function requireMonth(args) {
  if (!args || !args.dateFrom || !args.dateTo) {
    throw new Error("Выберите месяц");
  }
  if (String(args.dateFrom) > String(args.dateTo)) {
    throw new Error("Начало периода не может быть позже окончания");
  }
}

function daysInRange(dateFrom, dateTo) {
  const fromParts = String(dateFrom).slice(0, 10).split("-").map(Number);
  const toParts = String(dateTo).slice(0, 10).split("-").map(Number);
  let year = fromParts[0];
  let month = fromParts[1];
  let day = fromParts[2];
  const days = [];
  while (days.length < 40) {
    const key = `${year}-${String(month).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
    days.push(key);
    if (year === toParts[0] && month === toParts[1] && day === toParts[2]) break;
    day += 1;
    const last = new Date(year, month, 0).getDate();
    if (day > last) {
      day = 1;
      month += 1;
      if (month > 12) {
        month = 1;
        year += 1;
      }
    }
  }
  return days;
}

function pct(part, whole) {
  const w = num(whole);
  if (w <= 0) return 0;
  return (num(part) / w) * 100;
}

function vrOf(row) {
  if (!row) return 0;
  const opens = num(row.open_card);
  if (opens > 0) return num(row.vis_weight) / opens;
  const n = num(row.n);
  if (n > 0) return num(row.vis_sum) / n;
  return 0;
}

const METRICS = [
  { key: "vr", label: "VR" },
  { key: "ctr", label: "CTR" },
  { key: "cr_cart", label: "CR в корзину" },
  { key: "cr_cart_order", label: "CR корз-заказ" }
];

function indexByCatDay(rows) {
  const map = Object.create(null);
  for (const row of rows || []) {
    const category = text(row.category) || "Без категории";
    const day = text(row.day).slice(0, 10);
    if (!day) continue;
    if (!map[category]) map[category] = Object.create(null);
    map[category][day] = row;
  }
  return map;
}

function emptyFunnel() {
  return { open_count: 0, cart_count: 0, order_count: 0 };
}

function emptySearch() {
  return { open_card: 0, vis_weight: 0, vis_sum: 0, n: 0 };
}

function emptyAdvert() {
  return { views: 0, clicks: 0 };
}

function addInto(target, src, keys) {
  for (const key of keys) target[key] = num(target[key]) + num(src[key]);
}

function categoryRank(funnelMap, searchMap, advertMap) {
  const names = new Set([
    ...Object.keys(funnelMap),
    ...Object.keys(searchMap),
    ...Object.keys(advertMap)
  ]);
  const ranked = [];
  for (const name of names) {
    let opens = 0;
    const days = funnelMap[name] || {};
    for (const day of Object.keys(days)) opens += num(days[day].open_count);
    ranked.push({ name, opens });
  }
  ranked.sort((a, b) => {
    if (a.name === "Без категории") return 1;
    if (b.name === "Без категории") return -1;
    if (b.opens !== a.opens) return b.opens - a.opens;
    return a.name.localeCompare(b.name, "ru");
  });
  return ranked.map((item) => item.name);
}

function dayValues(days, pick) {
  return days.map((day) => pick(day));
}

export async function loadCabinets(_args, host) {
  const [rows, freshnessRows] = await Promise.all([
    host.db.queryResource("cabinets", []),
    host.db.queryResource("freshness", [])
  ]);
  const lastDate = freshnessRows.length ? text(freshnessRows[0].last_date) : "";
  return {
    rows: rows.map((row) => ({ id: text(row.id), name: text(row.name) })),
    suggested_month: lastDate.length >= 7 ? lastDate.slice(0, 7) : ""
  };
}

export async function loadTable(args, host) {
  requireMonth(args);
  const conn = cabinet(args);
  const dateFrom = String(args.dateFrom);
  const dateTo = String(args.dateTo);
  const days = daysInRange(dateFrom, dateTo);

  const [funnelRows, searchRows, advertRows] = await Promise.all([
    host.db.queryResource("funnelByCategory", [dateFrom, dateTo, conn, conn]),
    host.db.queryResource("searchByCategory", [dateFrom, dateTo, conn, conn]),
    host.db.queryResource("advertByCategory", [dateFrom, dateTo, conn, conn])
  ]);

  const funnel = indexByCatDay(funnelRows);
  const search = indexByCatDay(searchRows);
  const advert = indexByCatDay(advertRows);
  const categories = categoryRank(funnel, search, advert);
  const rows = [];

  for (const category of categories) {
    const funnelDays = funnel[category] || {};
    const searchDays = search[category] || {};
    const advertDays = advert[category] || {};
    const monthFunnel = emptyFunnel();
    const monthSearch = emptySearch();
    const monthAdvert = emptyAdvert();
    for (const day of days) {
      if (funnelDays[day]) addInto(monthFunnel, funnelDays[day], ["open_count", "cart_count", "order_count"]);
      if (searchDays[day]) addInto(monthSearch, searchDays[day], ["open_card", "vis_weight", "vis_sum", "n"]);
      if (advertDays[day]) addInto(monthAdvert, advertDays[day], ["views", "clicks"]);
    }
    const totals = {
      vr: vrOf(monthSearch),
      ctr: pct(monthAdvert.clicks, monthAdvert.views),
      cr_cart: pct(monthFunnel.cart_count, monthFunnel.open_count),
      cr_cart_order: pct(monthFunnel.order_count, monthFunnel.cart_count)
    };
    for (const metric of METRICS) {
      const values = dayValues(days, (day) => {
        if (metric.key === "vr") return vrOf(searchDays[day]);
        if (metric.key === "ctr") {
          const row = advertDays[day];
          return row ? pct(row.clicks, row.views) : 0;
        }
        if (metric.key === "cr_cart") {
          const row = funnelDays[day];
          return row ? pct(row.cart_count, row.open_count) : 0;
        }
        const row = funnelDays[day];
        return row ? pct(row.order_count, row.cart_count) : 0;
      });
      rows.push({
        category,
        metric: metric.key,
        label: metric.label,
        values,
        total: totals[metric.key]
      });
    }
  }

  return { dateFrom, dateTo, days, rows };
}
