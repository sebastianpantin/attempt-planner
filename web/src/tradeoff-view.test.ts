import { describe, expect, it } from "vitest";
import type { PlanShape, TradeoffPoint } from "../pkg/attempt_sim_wasm.js";
import {
  bestForGoalLabel,
  comparisonRows,
  highlightedPoints,
  noGoalPickNote,
  planLines,
  plannedAttempts,
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
      expect(Math.min(...gaps)).toBeGreaterThanOrEqual(0.08);
    }
    expect(riskAxis(0.155).ticks).toEqual([
      0, 0.002, 0.01, 0.02, 0.05, 0.1, 0.15, 0.2,
    ]);
  });
});

it("explains a missing best plan only when the cap ruled every plan out", () => {
  expect(noGoalPickNote(550, 0.05, undefined)).toBe(
    "No plan in the family keeps the bomb-out chance within 5%, so none is marked best for the goal. Raise the highest bomb-out chance in step 1 to see one.",
  );
  expect(noGoalPickNote(550, 0.05, 3)).toBe("");
  expect(noGoalPickNote(undefined, 0.05, undefined)).toBe("");
  expect(noGoalPickNote(550, undefined, undefined)).toBe("");
});

describe("comparisonRows", () => {
  const plan = (risk: number, totalKg: number, goal?: number) => ({
    ...point(80, risk, false),
    expectedTotalKg: totalKg,
    ...(goal === undefined ? {} : { goalProbability: goal }),
  });

  it("picks the plan with the highest average total in each risk band", () => {
    const points = [
      plan(0.0005, 540),
      plan(0.004, 545),
      plan(0.009, 552),
      plan(0.015, 556),
      plan(0.018, 550),
      plan(0.04, 530),
      plan(0.09, 520),
      plan(0.2, 571),
    ];
    expect(
      comparisonRows(points, undefined).map((row) => [
        row.label,
        row.point.bombOutProbability,
      ]),
    ).toEqual([
      ["Up to 0.5% bomb-out", 0.004],
      ["0.5% to 1% bomb-out", 0.009],
      ["1% to 2% bomb-out", 0.015],
      ["2% to 5% bomb-out", 0.04],
      ["5% to 10% bomb-out", 0.09],
    ]);
  });

  it("picks by the chance of the goal when there is one, and skips empty bands", () => {
    const points = [plan(0.001, 560, 0.4), plan(0.002, 550, 0.6)];
    expect(comparisonRows(points, undefined).map((row) => row.point)).toEqual([
      points[1],
    ]);
  });

  it("marks the best plan for the goal, adding it when no band picked it", () => {
    const points = [plan(0.001, 550, 0.4), plan(0.003, 549, 0.3)];
    expect(
      comparisonRows(points, 1).map((row) => [row.label, row.bestForGoal]),
    ).toEqual([
      ["Up to 0.5% bomb-out", false],
      ["Best for the goal", true],
    ]);
    expect(
      comparisonRows(points, 0).map((row) => [row.label, row.bestForGoal]),
    ).toEqual([["Up to 0.5% bomb-out", true]]);
  });

  it("has no rows without plans", () => {
    expect(comparisonRows([], undefined)).toEqual([]);
  });
});

describe("planned attempts", () => {
  const plan = {
    openerKg: 182.5,
    second: { afterMakeKg: 12.5, afterMissKg: 0 },
    third: { afterMakeKg: 7.5, afterMissKg: 0 },
  };

  it("lists the attempts after makes without a needless .0", () => {
    expect(plannedAttempts(plan)).toBe("182.5 / 195 / 202.5");
  });

  it("gives one line per lift", () => {
    expect(planLines({ squat: plan, bench: plan, deadlift: plan })).toEqual([
      "Squat 182.5 / 195 / 202.5",
      "Bench 182.5 / 195 / 202.5",
      "Deadlift 182.5 / 195 / 202.5",
    ]);
  });
});
