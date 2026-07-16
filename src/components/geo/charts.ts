/**
 * GEO 图表原语（dataviz：细笔触、2px 线、≥8px 点、留白、悬停层）。
 * 返回 SVG 字符串（供 v-html）。悬停用 data-chart/data-i 交给壳层做事件委托，
 * 避免 window 全局函数污染。
 */
import { MOCK } from "./data";

interface Series { name: string; color: string; raw: string; v: number[] }
interface AreaData { days: string[]; series: Series[] }
interface LineData { weeks: string[]; series: Series[] }

/* 图表几何：绘制与 tooltip 十字准线共用同一份常量（viewBox 坐标系）。 */
export const GEO_AREA = { W: 920, H: 240, ml: 46, mr: 16, mt: 14, mb: 28 };
export const GEO_LINE = { W: 920, H: 240, ml: 40, mr: 54, mt: 14, mb: 28 };

/* 堆叠面积图 + 十字准线（3 系列：自然/AI引荐/爬虫） */
export function stackedArea(d: AreaData, id: string): string {
  const { W, H, ml, mr, mt, mb } = GEO_AREA, iw = W - ml - mr, ih = H - mt - mb;
  const n = d.days.length;
  const totals = d.days.map((_, i) => d.series.reduce((s, se) => s + se.v[i], 0));
  const max = Math.ceil(Math.max(...totals) / 500) * 500;
  const X = (i: number) => ml + (n === 1 ? iw / 2 : (i * iw) / (n - 1));
  const Y = (v: number) => mt + ih - (v / max) * ih;
  let cum = new Array(n).fill(0), paths = "";
  d.series.forEach((se) => {
    const top = se.v.map((v, i) => cum[i] + v);
    const up = top.map((v, i) => `${i ? "L" : "M"}${X(i).toFixed(1)},${Y(v).toFixed(1)}`).join("");
    const dn = cum.map((v, i) => `L${X(n - 1 - i).toFixed(1)},${Y(cum[n - 1 - i]).toFixed(1)}`).join("");
    paths += `<path d="${up}${dn}Z" fill="${se.raw}" fill-opacity=".78" stroke="${se.raw}" stroke-width="1"/>`;
    cum = top;
  });
  let g = "";
  for (let t = 0; t <= max; t += max / 4) g += `<line class="gridline" x1="${ml}" y1="${Y(t)}" x2="${W - mr}" y2="${Y(t)}"/><text class="axis-lab" x="${ml - 6}" y="${Y(t) + 3}" text-anchor="end">${t >= 1000 ? t / 1000 + "k" : t}</text>`;
  let xl = "";
  d.days.forEach((dd, i) => { if (i % 3 === 0 || i === n - 1) xl += `<text class="axis-lab" x="${X(i)}" y="${H - 8}" text-anchor="middle">${dd}</text>`; });
  let lab = "";
  let cc = new Array(n).fill(0);
  d.series.forEach((se) => {
    const mid = cc[n - 1] + se.v[n - 1] / 2;
    cc = cc.map((v, i) => v + se.v[i]);
    lab += `<text x="${X(n - 1) - 6}" y="${Y(mid) + 3}" text-anchor="end" font-size="10" fill="var(--ink)" style="paint-order:stroke;stroke:var(--bg);stroke-width:3px">${se.name}</text>`;
  });
  const hot = d.days.map((_, i) => `<rect x="${X(i) - iw / (n - 1) / 2}" y="${mt}" width="${iw / (n - 1)}" height="${ih}" fill="transparent" data-chart="traffic" data-i="${i}"/>`).join("");
  return `<svg viewBox="0 0 ${W} ${H}" role="img" aria-label="每日流量三层堆叠">
    ${g}${paths}${lab}
    <line class="baseline" x1="${ml}" y1="${mt + ih}" x2="${W - mr}" y2="${mt + ih}"/>${xl}
    <line id="${id}-cross" x1="0" y1="${mt}" x2="0" y2="${mt + ih}" stroke="var(--ink)" stroke-width="1" stroke-dasharray="3 3" opacity="0"/>
    ${hot}</svg>`;
}

/* 多系列折线 + 十字准线（5 系列 → 图例，不逐点标注） */
export function lineChart(d: LineData, id: string): string {
  const { W, H, ml, mr, mt, mb } = GEO_LINE, iw = W - ml - mr, ih = H - mt - mb;
  const n = d.weeks.length, max = 10;
  const X = (i: number) => ml + (i * iw) / (n - 1);
  const Y = (v: number) => mt + ih - (v / max) * ih;
  let g = "";
  for (let t = 0; t <= max; t += 2.5) g += `<line class="gridline" x1="${ml}" y1="${Y(t)}" x2="${W - mr}" y2="${Y(t)}"/><text class="axis-lab" x="${ml - 6}" y="${Y(t) + 3}" text-anchor="end">${t}%</text>`;
  let ls = "";
  d.series.forEach((se) => {
    ls += `<path d="${se.v.map((v, i) => `${i ? "L" : "M"}${X(i)},${Y(v)}`).join("")}" fill="none" stroke="${se.raw}" stroke-width="2" stroke-linejoin="round" stroke-linecap="round"/>`;
    se.v.forEach((v, i) => { ls += `<circle cx="${X(i)}" cy="${Y(v)}" r="4" fill="${se.raw}" stroke="var(--card)" stroke-width="2"/>`; });
    ls += `<text x="${X(n - 1) + 7}" y="${Y(se.v[n - 1]) + 3}" font-size="10" fill="${se.raw}">${se.name}</text>`;
  });
  const xl = d.weeks.map((w, i) => `<text class="axis-lab" x="${X(i)}" y="${H - 8}" text-anchor="middle">${w}</text>`).join("");
  const hot = d.weeks.map((_, i) => `<rect x="${X(i) - iw / (n - 1) / 2}" y="${mt}" width="${iw / (n - 1)}" height="${ih}" fill="transparent" data-chart="engines" data-i="${i}"/>`).join("");
  return `<svg viewBox="0 0 ${W} ${H}" role="img" aria-label="五引擎品类词提及率周趋势">
    ${g}${ls}<line class="baseline" x1="${ml}" y1="${mt + ih}" x2="${W - mr}" y2="${mt + ih}"/>${xl}
    <line id="${id}-cross" x1="0" y1="${mt}" x2="0" y2="${mt + ih}" stroke="var(--ink)" stroke-width="1" stroke-dasharray="3 3" opacity="0"/>
    ${hot}</svg>`;
}

export function legend(series: Series[]): string {
  return `<div class="legend">` + series.map((s) => `<span><i style="background:${s.raw}"></i>${s.name}</span>`).join("") + `</div>`;
}

/* ── 壳层悬停：给定 chart+i 生成 tooltip 内容 HTML，并返回准线 X（viewBox 坐标） ── */
export function chartTip(chart: string, i: number): { html: string; crossId: string; x: number } | null {
  if (chart === "traffic") {
    const d = MOCK.traffic, { ml, mr, W } = GEO_AREA, iw = W - ml - mr, n = d.days.length;
    const x = ml + (i * iw) / (n - 1);
    const tot = d.series.reduce((s, se) => s + se.v[i], 0);
    const html =
      `<b>${d.days[i]}</b><br>` +
      d.series.map((se) => `<i style="background:${se.raw}"></i><span class="k">${se.name}</span> <b>${se.v[i].toLocaleString()}</b>`).join("<br>") +
      `<br><span class="k">合计</span> <b>${tot.toLocaleString()}</b> · <span class="k">AI 引荐占</span> <b>${((d.series[1].v[i] / tot) * 100).toFixed(1)}%</b>`;
    return { html, crossId: "traffic-cross", x };
  }
  if (chart === "engines") {
    const d = MOCK.engines, { ml, mr, W } = GEO_LINE, iw = W - ml - mr, n = d.weeks.length;
    const x = ml + (i * iw) / (n - 1);
    const html =
      `<b>${d.weeks[i]} 提及率</b><br>` +
      d.series.map((se) => `<i style="background:${se.raw}"></i><span class="k">${se.name}</span> <b>${se.v[i]}%</b>`).join("<br>");
    return { html, crossId: "engines-cross", x };
  }
  return null;
}
