import type { ChartHit, ChartMark, ChartScene } from "../src/types";

export interface MonthTotal {
  readonly month: string;
  readonly amount: number;
}

export const months: readonly MonthTotal[] = [
  { month: "Jun", amount: 21.4 },
  { month: "Jul", amount: 19.8 },
  { month: "Aug", amount: 18.9 },
  { month: "Sep", amount: 22.6 },
  { month: "Oct", amount: 35.3 },
  { month: "Nov", amount: 41.7 },
];

const width = 480;
const height = 196;
const left = 34;
const right = 476;
const top = 18;
const bottom = 164;
const ceiling = 45;
const ticks = [0, 15, 30, 45];

function yOf(value: number): number {
  return bottom - (value / ceiling) * (bottom - top);
}

export function billsScene(): ChartScene {
  const band = (right - left) / months.length;
  const barWidth = Math.min(40, band * 0.56);
  const marks: ChartMark[] = [];
  const hits: ChartHit[] = [];

  for (const tick of ticks) {
    const y = yOf(tick);
    marks.push({ type: "line", x1: left, y1: y, x2: right, y2: y, stroke: tick === 0 ? "axis" : "grid", stroke_width: 1 });
    marks.push({ type: "text", x: left - 8, y: y + 4, text: tick === 0 ? "0" : `${tick}k`, anchor: "end", role: "tick", color: "muted", size: 11 });
  }

  months.forEach((entry, index) => {
    const center = left + band * index + band / 2;
    const y = yOf(entry.amount);
    const latest = index === months.length - 1;
    marks.push({ type: "rect", x: center - barWidth / 2, y, width: barWidth, height: bottom - y, fill: latest ? "series-1" : "series-bar" });
    marks.push({ type: "text", x: center, y: y - 7, text: entry.amount.toFixed(1), anchor: "middle", role: latest ? "title" : "value", color: latest ? "text" : "muted", size: 11 });
    marks.push({ type: "text", x: center, y: bottom + 20, text: entry.month, anchor: "middle", role: "tick", color: "muted", size: 12 });
    hits.push({ shape: "rect", row: index, label: `${entry.month}: ${entry.amount.toFixed(1)} thousand so'm`, x: left + band * index + 3, y: top - 12, width: band - 6, height: bottom - top + 12 });
  });

  return { view_box: { x: 0, y: 0, width, height }, kind: "bar", title: "Last six months", description: "Bills rose from 21.4 thousand so'm in June to 41.7 thousand in November.", marks, legend: [], hits };
}
