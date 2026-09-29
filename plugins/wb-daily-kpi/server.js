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

function dateEnd(dateTo) {
  return `${String(dateTo)}T23:59:59`;
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

function indexByDay(rows) {
  const map = Object.create(null);
  for (const row of rows || []) {
    const day = text(row.day).slice(0, 10);
    if (day) map[day] = row;
  }
  return map;
}

function ratio(part, whole) {
  const w = num(whole);
  if (w <= 0) return 0;
  return num(part) / w;
}

function pct(part, whole) {
  return ratio(part, whole) * 100;
}

function emptySales() {
  return {
    buyout_qty: 0,
    buyout_fbo: 0,
    buyout_fbs: 0,
    buyout_dbs: 0,
    buyout_dbw: 0,
    buyout_dbs_dbw: 0,
    revenue_rub: 0,
    return_qty: 0,
    return_sum: 0,
    cost_incl_returns: 0
  };
}

function emptyOrders() {
  return {
    order_qty: 0,
    order_fbo: 0,
    order_fbs: 0,
    order_dbs: 0,
    order_dbw: 0,
    order_dbs_dbw: 0,
    order_rub: 0
  };
}

function almostEqual(a, b) {
  return Math.abs(num(a) - num(b)) < 0.05;
}

function emptyAdvert() {
  return { views: 0, clicks: 0, ad_spend: 0, atbs: 0 };
}

function emptyFunnel() {
  return { open_count: 0, cart_count: 0, funnel_order_count: 0 };
}

function mergeDay(day, sales, orders, advert, funnel, dailyPlan) {
  const s = sales || emptySales();
  const o = orders || emptyOrders();
  const a = advert || emptyAdvert();
  const f = funnel || emptyFunnel();
  const buyoutQty = num(s.buyout_qty);
  const revenue = num(s.revenue_rub);
  const returnSum = num(s.return_sum);
  const cost = num(s.cost_incl_returns);
  const netRevenue = revenue - returnSum;
  const grossProfit = netRevenue - cost;
  const orderQty = num(o.order_qty);
  const orderRub = num(o.order_rub);
  const views = num(a.views);
  const clicks = num(a.clicks);
  const adSpend = num(a.ad_spend);
  return {
    day,
    plan_rub: dailyPlan,
    forecast_rub: 0,
    buyout_qty: buyoutQty,
    buyout_fbo: num(s.buyout_fbo),
    buyout_fbs: num(s.buyout_fbs),
    buyout_dbs: num(s.buyout_dbs),
    buyout_dbw: num(s.buyout_dbw),
    buyout_dbs_dbw: num(s.buyout_dbs) + num(s.buyout_dbw),
    revenue_rub: revenue,
    buyout_avg: ratio(revenue, buyoutQty),
    order_qty: orderQty,
    order_fbo: num(o.order_fbo),
    order_fbs: num(o.order_fbs),
    order_dbs: num(o.order_dbs),
    order_dbw: num(o.order_dbw),
    order_dbs_dbw: num(o.order_dbs) + num(o.order_dbw),
    order_rub: orderRub,
    order_avg_check: ratio(orderRub, orderQty),
    gross_profit: grossProfit,
    gross_margin: pct(grossProfit, netRevenue),
    cost_incl_returns: cost,
    return_qty: num(s.return_qty),
    return_sum: returnSum,
    views,
    clicks,
    cpc: ratio(adSpend, clicks),
    ad_spend: adSpend,
    ctr: pct(clicks, views),
    cr_cart: pct(f.cart_count, f.open_count),
    cr_cart_order: pct(f.funnel_order_count, f.cart_count),
    drr: pct(adSpend, revenue),
    cps: ratio(adSpend, buyoutQty),
    open_count: num(f.open_count),
    cart_count: num(f.cart_count),
    funnel_order_count: num(f.funnel_order_count)
  };
}

function applyForecast(rows, today) {
  const todayKey = text(today).slice(0, 10);
  let elapsedRevenue = 0;
  let elapsedDays = 0;
  for (const row of rows) {
    if (!todayKey || row.day <= todayKey) {
      elapsedRevenue += row.revenue_rub;
      elapsedDays += 1;
    }
  }
  const avg = elapsedDays > 0 ? elapsedRevenue / elapsedDays : 0;
  for (const row of rows) {
    if (!todayKey || row.day <= todayKey) {
      row.forecast_rub = row.revenue_rub;
    } else {
      row.forecast_rub = avg;
    }
  }
}

function totalsOf(rows) {
  const t = mergeDay("ИТОГО", emptySales(), emptyOrders(), emptyAdvert(), emptyFunnel(), 0);
  let openCount = 0;
  let cartCount = 0;
  let funnelOrders = 0;
  for (const row of rows) {
    t.plan_rub += row.plan_rub;
    t.forecast_rub += row.forecast_rub;
    t.buyout_qty += row.buyout_qty;
    t.buyout_fbo += row.buyout_fbo;
    t.buyout_fbs += row.buyout_fbs;
    t.buyout_dbs += row.buyout_dbs;
    t.buyout_dbw += row.buyout_dbw;
    t.buyout_dbs_dbw += row.buyout_dbs_dbw;
    t.revenue_rub += row.revenue_rub;
    t.order_qty += row.order_qty;
    t.order_fbo += row.order_fbo;
    t.order_fbs += row.order_fbs;
    t.order_dbs += row.order_dbs;
    t.order_dbw += row.order_dbw;
    t.order_dbs_dbw += row.order_dbs_dbw;
    t.order_rub += row.order_rub;
    t.gross_profit += row.gross_profit;
    t.cost_incl_returns += row.cost_incl_returns;
    t.return_qty += row.return_qty;
    t.return_sum += row.return_sum;
    t.views += row.views;
    t.clicks += row.clicks;
    t.ad_spend += row.ad_spend;
    openCount += row.open_count;
    cartCount += row.cart_count;
    funnelOrders += row.funnel_order_count;
  }
  const netRevenue = t.revenue_rub - t.return_sum;
  t.buyout_avg = ratio(t.revenue_rub, t.buyout_qty);
  t.order_avg_check = ratio(t.order_rub, t.order_qty);
  t.gross_margin = pct(t.gross_profit, netRevenue);
  t.cpc = ratio(t.ad_spend, t.clicks);
  t.ctr = pct(t.clicks, t.views);
  t.cr_cart = pct(cartCount, openCount);
  t.cr_cart_order = pct(funnelOrders, cartCount);
  t.drr = pct(t.ad_spend, t.revenue_rub);
  t.cps = ratio(t.ad_spend, t.buyout_qty);
  return t;
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
  const until = dateEnd(dateTo);
  const monthPlan = Math.max(0, num(args.monthPlan));
  const days = daysInRange(dateFrom, dateTo);
  const dailyPlan = days.length ? monthPlan / days.length : 0;

  const [salesRows, orderRows, advertRows, funnelRows] = await Promise.all([
    host.db.queryResource("salesDaily", [dateFrom, until, conn, conn]),
    host.db.queryResource("ordersDaily", [dateFrom, until, conn, conn]),
    host.db.queryResource("advertDaily", [dateFrom, dateTo, conn, conn]),
    host.db.queryResource("funnelDaily", [dateFrom, dateTo, conn, conn])
  ]);

  const sales = indexByDay(salesRows);
  const orders = indexByDay(orderRows);
  const advert = indexByDay(advertRows);
  const funnel = indexByDay(funnelRows);
  const rows = days.map((day) => mergeDay(day, sales[day], orders[day], advert[day], funnel[day], dailyPlan));
  applyForecast(rows, args.today);
  const totals = totalsOf(rows);
  const buyoutSplit = totals.buyout_fbo + totals.buyout_fbs + totals.buyout_dbs_dbw;
  const orderSplit = totals.order_fbo + totals.order_fbs + totals.order_dbs_dbw;
  return {
    dateFrom,
    dateTo,
    monthPlan,
    rows,
    totals,
    checks: {
      buyout_split_ok: almostEqual(totals.buyout_qty, buyoutSplit),
      order_split_ok: almostEqual(totals.order_qty, orderSplit),
      buyout_dbs_dbw_ok: almostEqual(totals.buyout_dbs_dbw, totals.buyout_dbs + totals.buyout_dbw),
      order_dbs_dbw_ok: almostEqual(totals.order_dbs_dbw, totals.order_dbs + totals.order_dbw),
      buyout_avg_ok: almostEqual(totals.buyout_avg, ratio(totals.revenue_rub, totals.buyout_qty)),
      order_avg_ok: almostEqual(totals.order_avg_check, ratio(totals.order_rub, totals.order_qty))
    }
  };
}
