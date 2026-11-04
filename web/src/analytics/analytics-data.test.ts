import { describe, expect, it } from "vitest";
import type {
  AttemptPath,
  Change,
  Evaluation,
  LiftPlanKg,
  LiftSummary,
  PerLift,
  PlanOutcome,
} from "../../pkg/attempt_sim_wasm.js";
import {
  attemptView,
  bombOutRisk,
  chanceOfAtLeast,
  dayView,
  delta,
  describeChange,
  histogram,
  reachCurve,
  whatIfs,
} from "./analytics-data.ts";

const totals = [
  { totalKg: 0, share: 0.1 },
  { totalKg: 500, share: 0.2 },
  { totalKg: 502.5, share: 0.1 },
  { totalKg: 507.5, share: 0.3 },
  { totalKg: 520, share: 0.3 },
];

describe("histogram", () => {
  it("bins totals by 5 kg from the goal, with empty bins and bomb-outs apart", () => {
    const { bombOutShare, bins } = histogram(totals, 505);
    expect(bombOutShare).toBe(0.1);
    expect(bins.map((bin) => [bin.fromKg, bin.reachesGoal])).toEqual([
      [500, false],
      [505, true],
      [510, true],
      [515, true],
      [520, true],
    ]);
    expect(bins.map((bin) => bin.share)).toEqual([
      expect.closeTo(0.3),
      0.3,
      0,
      0,
      0.3,
    ]);
  });

  it("marks no bin without a goal", () => {
    const { bins } = histogram(totals);
    expect(bins.map((bin) => bin.fromKg)).toEqual([500, 505, 510, 515, 520]);
    expect(bins.some((bin) => bin.reachesGoal)).toBe(false);
  });

  it("widens the bins over a wide range of totals", () => {
    const { bins } = histogram([
      { totalKg: 300, share: 0.5 },
      { totalKg: 600, share: 0.5 },
    ]);
    expect(bins[0]!.toKg - bins[0]!.fromKg).toBe(20);
  });

  it("has no bins when every meet bombs out", () => {
    expect(histogram([{ totalKg: 0, share: 1 }])).toEqual({
      bombOutShare: 1,
      bins: [],
    });
  });
});

describe("chance of reaching a total", () => {
  it("counts every total at or above each one", () => {
    expect(reachCurve(totals)).toEqual([
      { totalKg: 500, chance: expect.closeTo(0.9) },
      { totalKg: 502.5, chance: expect.closeTo(0.7) },
      { totalKg: 507.5, chance: expect.closeTo(0.6) },
      { totalKg: 520, chance: expect.closeTo(0.3) },
    ]);
  });

  it("gives the chance at a goal between totals", () => {
    expect(chanceOfAtLeast(totals, 505)).toBeCloseTo(0.6);
  });
});

const path = (
  made: boolean[],
  share: number,
  bestKg?: number,
): AttemptPath => ({
  made,
  share,
  ...(bestKg === undefined ? {} : { bestKg }),
});

const summary = (
  makeProbability: number,
  technicalBombOutProbability: number,
  paths: AttemptPath[] = [],
): LiftSummary => ({
  makeProbability,
  strengthMissRate: 0,
  technicalMissRate: 0,
  attemptMakeProbabilities: [0.98, 0.85, 0.6],
  technicalBombOutProbability,
  paths,
});

describe("bomb-out risk", () => {
  it("splits each lift's risk by cause and names the riskiest lift", () => {
    const risk = bombOutRisk({
      squat: summary(0.99, 0),
      bench: summary(0.98, 0.005),
      deadlift: summary(0.99, 0.002),
    });
    expect(risk.lifts[1]).toEqual({
      lift: "bench",
      total: expect.closeTo(0.02),
      technical: 0.005,
      tooHeavy: expect.closeTo(0.015),
    });
    expect(risk.scaleMax).toBe(0.02);
    expect(risk.hint).toBe("Bench carries 50.0% of the bomb-out risk.");
  });

  it("says so when no lift bombs out", () => {
    const risk = bombOutRisk({
      squat: summary(1, 0),
      bench: summary(1, 0),
      deadlift: summary(1, 0),
    });
    expect(risk.hint).toBe("No lift bombed out in the simulated meets.");
  });
});

const paths = [
  path([true, true, true], 0.5, 202.5),
  path([true, true, false], 0.3, 195),
  path([true, false, false], 0.15, 182.5),
  path([false, true, true], 0.03, 195),
  path([false, false, true], 0.02, 182.5),
  path([false, false, false], 0),
  path([true, false, true], 0, 195),
  path([false, true, false], 0, 182.5),
];

it("shows the paths after a made opener and groups those after a miss", () => {
  const view = attemptView(summary(1, 0, paths));
  expect(view.makeChances).toEqual([0.98, 0.85, 0.6]);
  expect(view.rows.map((row) => row.bestKg)).toEqual([202.5, 195, 182.5]);
  expect(view.missedOpenerShare).toBeCloseTo(0.05);
});

it("reads a bad, normal and good day and the best total the plan allows", () => {
  const evaluation = {
    totalPercentilesKg: { p10: 532.5, p50: 557.5, p90: 575 },
    lifts: {
      squat: summary(1, 0, paths),
      bench: summary(1, 0, [path([true, true, true], 1, 140)]),
      deadlift: summary(1, 0, [path([false, false, false], 1)]),
    },
  } as unknown as Evaluation;
  expect(dayView(evaluation)).toEqual({
    badKg: 532.5,
    normalKg: 557.5,
    goodKg: 575,
    bestPossibleKg: 342.5,
  });
});

const plan: LiftPlanKg = {
  openerKg: 220,
  second: { afterMakeKg: 12.5, afterMissKg: 0 },
  third: { afterMakeKg: 7.5, afterMissKg: 0 },
};
const plans: PerLift<LiftPlanKg> = { squat: plan, bench: plan, deadlift: plan };

const outcome = (
  goalProbability: number,
  bombOutProbability: number,
  expectedTotalKg = 540,
): PlanOutcome => ({
  plans,
  goalProbability,
  bombOutProbability,
  expectedTotalKg,
});

const change = (
  lift: Change["lift"],
  setting: Change["setting"],
  weightKg: number,
  result: PlanOutcome,
): Change => ({ lift, setting, weightKg, result });

describe("what-ifs", () => {
  it("describes a jump and an opener", () => {
    const current = outcome(0.7, 0.04);
    expect(
      describeChange(change("bench", "thirdAfterMake", 5, current), current),
    ).toBe("Bench 3rd: +5.0 kg instead of +7.5 kg after a make");
    expect(
      describeChange(change("deadlift", "opener", 215, current), current),
    ).toBe("Deadlift opener: 215.0 kg instead of 220.0 kg");
  });

  it("signs a delta and says whether it helps", () => {
    expect(delta("goal", 0.021, 1)).toEqual({
      text: "goal +2.1%",
      direction: "better",
    });
    expect(delta("bomb-out", 0.001, -1)).toEqual({
      text: "bomb-out +0.1%",
      direction: "worse",
    });
    expect(delta("bomb-out", -0.0001, -1)).toEqual({
      text: "bomb-out same",
      direction: "same",
    });
  });

  it("takes the improving changes first, then the safest", () => {
    const current = outcome(0.7, 0.04);
    const picks = whatIfs(current, [
      change("bench", "thirdAfterMake", 5, outcome(0.72, 0.04)),
      change("squat", "opener", 222.5, outcome(0.69, 0.05)),
      change("deadlift", "opener", 215, outcome(0.6, 0.031)),
      change("squat", "opener", 217.5, outcome(0.65, 0.035)),
    ]);
    expect(picks.map((pick) => pick.text)).toEqual([
      "Bench 3rd: +5.0 kg instead of +7.5 kg after a make",
      "Deadlift opener: 215.0 kg instead of 220.0 kg",
      "Squat opener: 217.5 kg instead of 220.0 kg",
    ]);
    expect(picks[0]!.goal).toEqual({ text: "goal +2.0%", direction: "better" });
    expect(picks[1]!.bombOut).toEqual({
      text: "bomb-out −0.9%",
      direction: "better",
    });
  });

  it("ranks a plan within the bomb-out cap above a likelier one over it", () => {
    const current = outcome(0.7, 0.05);
    const picks = whatIfs(
      current,
      [
        change("squat", "opener", 222.5, outcome(0.8, 0.06)),
        change("squat", "opener", 217.5, outcome(0.6, 0.03)),
      ],
      0.04,
    );
    expect(picks.map((pick) => pick.text)).toEqual([
      "Squat opener: 217.5 kg instead of 220.0 kg",
    ]);
  });

  it("leaves out the goal delta without a goal", () => {
    const current: PlanOutcome = {
      plans,
      bombOutProbability: 0.04,
      expectedTotalKg: 540,
    };
    const [pick] = whatIfs(current, [
      change("squat", "opener", 222.5, { ...current, expectedTotalKg: 541 }),
    ]);
    expect(pick!.goal).toBeUndefined();
    expect(pick!.plans).toBe(plans);
  });
});
