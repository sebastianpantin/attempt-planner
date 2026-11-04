import { describe, expect, it } from "vitest";
import type { PlanShape, TradeoffPoint } from "../pkg/attempt_sim_wasm.js";
import {
  bestForGoalLabel,
  highlightedPoints,
  MIN_TICK_GAP,
  riskAxis,
  shapeLabel,
} from "./tradeoff-view.ts";

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

it("names the bomb-out cap the best plan for the goal stays within", () => {
  expect(bestForGoalLabel(undefined)).toBe("best for the goal");
  expect(bestForGoalLabel(0.05)).toBe("best for the goal within 5% bomb-out");
});

describe("riskAxis", () => {
  it("ends on the first round tick at or above the riskiest plan", () => {
    expect(riskAxis(0.155).max).toBe(0.2);
    expect(riskAxis(0.2).max).toBe(0.2);
    expect(riskAxis(0).max).toBe(0.01);
    expect(riskAxis(2).max).toBe(1);
  });

  it("keeps the ticks far enough apart on the square-root axis", () => {
    for (const maxRisk of [0, 0.004, 0.03, 0.155, 0.35, 0.9]) {
      const { max, ticks } = riskAxis(maxRisk);
      expect(ticks[0]).toBe(0);
      expect(ticks.at(-1)).toBe(max);
      const positions = ticks.map((tick) => Math.sqrt(tick / max));
      const gaps = positions.slice(1).map((p, i) => p - (positions[i] ?? 0));
      expect(Math.min(...gaps)).toBeGreaterThanOrEqual(MIN_TICK_GAP);
    }
    expect(riskAxis(0.155).ticks).toEqual([
      0, 0.002, 0.01, 0.02, 0.05, 0.1, 0.15, 0.2,
    ]);
  });
});
