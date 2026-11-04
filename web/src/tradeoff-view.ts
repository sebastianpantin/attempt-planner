import type { PlanShape, TradeoffPoint } from "../pkg/attempt_sim_wasm.js";
import { finePercent } from "./format.ts";

export const shapeLabel = (shape: PlanShape) =>
  `${shape.openerPercent}% / ${shape.secondPercent}% / ${shape.thirdPercent}%`;

export interface HighlightedPoint {
  point: TradeoffPoint;
  bestForGoal: boolean;
}

/** The frontier plans plus the best plan for the goal, from safest to riskiest. */
export function highlightedPoints(
  points: TradeoffPoint[],
  bestForGoal: number | undefined,
): HighlightedPoint[] {
  return points
    .map((point, index) => ({ point, bestForGoal: index === bestForGoal }))
    .filter(({ point, bestForGoal }) => point.onFrontier || bestForGoal)
    .sort(
      (a, b) =>
        a.point.bombOutProbability - b.point.bombOutProbability ||
        b.point.expectedTotalKg - a.point.expectedTotalKg,
    );
}

/** What the best plan for the goal is best at, naming the bomb-out cap when there is one. */
export const bestForGoalLabel = (bombOutCap: number | undefined) =>
  bombOutCap === undefined
    ? "best for the goal"
    : `best for the goal within ${finePercent(bombOutCap)} bomb-out`;

const RISK_TICKS = [
  0, 0.001, 0.002, 0.005, 0.01, 0.02, 0.05, 0.1, 0.15, 0.2, 0.3, 0.5, 1,
];

/** The smallest gap between risk ticks, as a share of the axis length. */
export const MIN_TICK_GAP = 0.08;

/**
 * The end of a square-root risk axis, the first round tick at or above the riskiest plan
 * and at least 1%, and the round ticks along it, the larger ticks winning where two would
 * sit closer than `MIN_TICK_GAP`.
 */
export function riskAxis(maxRisk: number): { max: number; ticks: number[] } {
  const max = RISK_TICKS.find((tick) => tick >= Math.max(maxRisk, 0.01)) ?? 1;
  const position = (tick: number) => Math.sqrt(tick / max);
  const kept = [0];
  for (const tick of RISK_TICKS.filter(
    (tick) => tick > 0 && tick <= max,
  ).reverse()) {
    if (
      kept.every(
        (other) => Math.abs(position(tick) - position(other)) >= MIN_TICK_GAP,
      )
    ) {
      kept.push(tick);
    }
  }
  return { max, ticks: kept.sort((a, b) => a - b) };
}
