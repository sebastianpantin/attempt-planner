import type { PlanShape, TradeoffPoint } from "../pkg/attempt_sim_wasm.js";

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
