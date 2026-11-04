import type { Evaluation, TradeoffPoint } from "../pkg/attempt_sim_wasm.js";
import { axisBottom, axisLeft } from "d3-axis";
import { scaleLinear, scaleSequential, scaleSqrt } from "d3-scale";
import { interpolateViridis } from "d3-scale-chromatic";
import { select } from "d3-selection";
import { line, symbol, symbolCross } from "d3-shape";
import * as format from "./format.ts";
import { highlightedPoints, shapeLabel } from "./tradeoff-view.ts";

export interface ChartData {
  points: TradeoffPoint[];
  bestForGoal: number | undefined;
  current:
    Pick<Evaluation, "bombOutProbability" | "expectedTotalKg"> | undefined;
}

const WIDTH = 560;
const HEIGHT = 340;
const MARGIN = { top: 16, right: 16, bottom: 48, left: 64 };

export const goalColour = scaleSequential(interpolateViridis).domain([0, 1]);

export const describePoint = (point: TradeoffPoint) =>
  [
    shapeLabel(point.shape),
    `bomb-out ${format.percent(point.bombOutProbability)}`,
    `average total ${format.kg(point.expectedTotalKg)}`,
    ...(point.goalProbability === undefined
      ? []
      : [`goal ${format.percent(point.goalProbability)}`]),
  ].join(", ");

export function drawTradeoff(
  svg: SVGSVGElement,
  { points, bestForGoal, current }: ChartData,
  pick: (point: TradeoffPoint) => void,
) {
  const shown = [...points, ...(current ? [current] : [])];
  const risks = shown.map((p) => p.bombOutProbability);
  const totals = shown.map((p) => p.expectedTotalKg);
  const x = scaleSqrt()
    .domain([0, Math.max(...risks, 0.01)])
    .nice()
    .range([MARGIN.left, WIDTH - MARGIN.right]);
  const y = scaleLinear()
    .domain([Math.min(...totals) - 2.5, Math.max(...totals) + 2.5])
    .nice()
    .range([HEIGHT - MARGIN.bottom, MARGIN.top]);
  const at = (p: { bombOutProbability: number; expectedTotalKg: number }) =>
    `translate(${x(p.bombOutProbability)},${y(p.expectedTotalKg)})`;

  const root = select(svg).attr("viewBox", `0 0 ${WIDTH} ${HEIGHT}`);
  root.selectChildren().remove();

  root
    .append("g")
    .attr("transform", `translate(0,${HEIGHT - MARGIN.bottom})`)
    .call(
      axisBottom(x)
        .ticks(6)
        .tickFormat((value) => format.finePercent(Number(value))),
    );
  root
    .append("g")
    .attr("transform", `translate(${MARGIN.left},0)`)
    .call(
      axisLeft(y)
        .ticks(6)
        .tickFormat((value) => format.kg(Number(value))),
    );
  root
    .append("text")
    .attr("class", "axis-label")
    .attr("x", (MARGIN.left + WIDTH - MARGIN.right) / 2)
    .attr("y", HEIGHT - 8)
    .attr("text-anchor", "middle")
    .text("Chance of bombing out");
  root
    .append("text")
    .attr("class", "axis-label")
    .attr(
      "transform",
      `translate(14,${(MARGIN.top + HEIGHT - MARGIN.bottom) / 2}) rotate(-90)`,
    )
    .attr("text-anchor", "middle")
    .text("Average total");

  root
    .append("g")
    .attr("class", "family")
    .selectAll("circle")
    .data(points.filter((p) => !p.onFrontier))
    .join("circle")
    .attr("transform", at)
    .attr("r", 2.5);

  const highlighted = highlightedPoints(points, bestForGoal);
  const frontier = highlighted
    .filter(({ point }) => point.onFrontier)
    .map(({ point }) => point);
  root
    .append("path")
    .attr("class", "frontier")
    .attr(
      "d",
      line<TradeoffPoint>()
        .x((p) => x(p.bombOutProbability))
        .y((p) => y(p.expectedTotalKg))(frontier),
    );

  const best = highlighted.find((h) => h.bestForGoal)?.point;
  if (best) {
    root
      .append("circle")
      .attr("class", "best-for-goal")
      .attr("transform", at(best))
      .attr("r", 10);
  }

  root
    .append("g")
    .attr("class", "frontier-points")
    .selectAll("circle")
    .data(highlighted.map(({ point }) => point))
    .join("circle")
    .attr("transform", at)
    .attr("r", 4.5)
    .attr("fill", (p) =>
      p.goalProbability === undefined ? null : goalColour(p.goalProbability),
    )
    .on("click", (_, p) => pick(p))
    .append("title")
    .text((p) => `${describePoint(p)}. Click to use this plan.`);

  if (current) {
    root
      .append("path")
      .attr("class", "current")
      .attr("transform", at(current))
      .attr("d", symbol(symbolCross, 120)())
      .append("title")
      .text("Your plan");
  }
}
