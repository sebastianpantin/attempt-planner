import { expect, it } from "vitest";
import type { PlanShape, TradeoffPoint } from "../pkg/attempt_sim_wasm.js";
import { highlightedPoints, shapeLabel } from "./tradeoff-view.ts";

const point = (
  opener: number,
  bombOutProbability: number,
  onFrontier: boolean,
): TradeoffPoint => ({
  shape: { openerPercent: opener, secondPercent: 97, thirdPercent: 102 },
  plans: {} as TradeoffPoint["plans"],
  bombOutProbability,
  expectedTotalKg: 500 + opener,
  onFrontier,
});

it("labels a shape by its three percentages", () => {
  const shape: PlanShape = {
    openerPercent: 90,
    secondPercent: 97,
    thirdPercent: 102,
  };
  expect(shapeLabel(shape)).toBe("90% / 97% / 102%");
});

it("lists frontier plans from safest to riskiest", () => {
  const points = [
    point(92, 0.05, true),
    point(85, 0.01, true),
    point(88, 0.02, false),
  ];
  expect(
    highlightedPoints(points, undefined).map(
      (p) => p.point.shape.openerPercent,
    ),
  ).toEqual([85, 92]);
});

it("adds the best plan for the goal even when it is off the frontier", () => {
  const points = [point(92, 0.05, true), point(88, 0.02, false)];
  expect(highlightedPoints(points, 1)).toEqual([
    { point: points[1], bestForGoal: true },
    { point: points[0], bestForGoal: false },
  ]);
});
