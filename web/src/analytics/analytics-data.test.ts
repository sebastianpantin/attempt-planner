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
  appliedLine,
  attemptView,
  bombOutRisk,
  chanceOfAtLeast,
  dayView,
  delta,
  describeChange,
  histogram,
  histogramLabel,
  labelEvery,
  pathText,
  pathWeights,
  reachCurve,
  reachLabel,
  tallestBin,
  totalCheck,
  totalCheckRange,
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
  it("bins totals by 10 kg from the goal, with empty bins and bomb-outs apart", () => {
    const { bombOutShare, bins } = histogram(totals, 505);
    expect(bombOutShare).toBe(0.1);
    expect(bins.map((bin) => [bin.fromKg, bin.reachesGoal])).toEqual([
      [495, false],
      [505, true],
      [515, true],
    ]);
    expect(bins.map((bin) => bin.share)).toEqual([
      expect.closeTo(0.3),
      0.3,
      0.3,
    ]);
  });

  it("marks no bin without a goal", () => {
    const { bins } = histogram(totals);
    expect(bins.map((bin) => bin.fromKg)).toEqual([500, 510, 520]);
    expect(bins.map((bin) => bin.share)).toEqual([expect.closeTo(0.6), 0, 0.3]);
    expect(bins.some((bin) => bin.reachesGoal)).toBe(false);
  });

  it("evens out totals that alternate between common and rare 2.5 kg steps", () => {
    const alternating = Array.from({ length: 16 }, (_, index) => ({
      totalKg: 500 + index * 2.5,
      share: index % 2 === 0 ? 0.1 : 0.025,
    }));
    const { bins } = histogram(alternating, 500);
    expect(bins.map((bin) => bin.share)).toEqual([
      expect.closeTo(0.25),
      expect.closeTo(0.25),
      expect.closeTo(0.25),
      expect.closeTo(0.25),
    ]);
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

describe("the histogram's labels", () => {
  const { bins } = histogram(totals, 505);

  it("finds the tallest bin, the first of equals", () => {
    expect(tallestBin(bins)?.fromKg).toBe(495);
    expect(tallestBin([])).toBeUndefined();
  });

  it("labels about nine bins at most", () => {
    expect(labelEvery(0)).toBe(1);
    expect(labelEvery(9)).toBe(1);
    expect(labelEvery(10)).toBe(2);
    expect(labelEvery(24)).toBe(3);
  });

  it("describes the totals, their likeliest bin and the goal", () => {
    expect(
      histogramLabel(histogram(totals, 505), { kg: 505, chance: 0.6 }),
    ).toBe(
      "Histogram of simulated totals. Bomb-out 10.0%. Totals from 495 kg to 525 kg, most often 495 kg to 505 kg. 60.0% at or above the goal of 505 kg.",
    );
    expect(histogramLabel({ bombOutShare: 1, bins: [] })).toBe(
      "Histogram of simulated totals. Bomb-out 100.0%.",
    );
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

describe("checking any total", () => {
  const points = reachCurve(totals);

  it("reads the chance of reaching a total", () => {
    expect(totalCheck(totals, 507.5)).toBe("507.5 kg: 60.0%");
    expect(totalCheck(totals, 505)).toBe("505 kg: 60.0%");
  });

  it("runs over the totals that came up, starting on a 2.5 kg step within them", () => {
    expect(totalCheckRange(points, 511)).toEqual({
      min: 500,
      max: 520,
      value: 510,
    });
    expect(totalCheckRange(points, 600).value).toBe(520);
    expect(totalCheckRange(points, 0).value).toBe(500);
  });

  it("describes the curve and the goal", () => {
    expect(reachLabel(points, { kg: 505, chance: 0.6 })).toBe(
      "Chance of reaching each total. 90.0% reach 500 kg, falling to 30.0% at 520 kg. 60.0% reach the goal of 505 kg.",
    );
    expect(reachLabel([])).toBe(
      "Chance of reaching each total. Every simulated meet bombs out.",
    );
  });
});

describe("bomb-out risk", () => {
  it("splits each lift's own risk by cause and says the lifts overlap", () => {
    const risk = bombOutRisk(
      {
        squat: summary(0.99, 0),
        bench: summary(0.98, 0.005),
        deadlift: summary(0.99, 0.002),
      },
      0.035,
    );
    expect(risk.lifts[1]).toEqual({
      lift: "bench",
      total: expect.closeTo(0.02),
      technical: 0.005,
      tooHeavy: expect.closeTo(0.015),
    });
    expect(risk.scaleMax).toBe(0.02);
    expect(risk.overlap).toBe(
      "One meet can bomb out on two lifts, so these add up to 4.0%, more than your 3.5% chance of bombing out.",
    );
    expect(risk.hint).toBe(
      "Bench press is the riskiest lift: it bombs out in 2.0% of meets, against 3.5% for the meet as a whole. A lighter bench press opener is the cheapest fix.",
    );
  });

  it("says nothing about overlap when the lifts add up to the meet's chance", () => {
    const risk = bombOutRisk(
      {
        squat: summary(0.99, 0),
        bench: summary(0.995, 0),
        deadlift: summary(1, 0),
      },
      0.015,
    );
    expect(risk.overlap).toBe("");
  });

  it("points to technique when most of the riskiest lift's risk is technical", () => {
    const risk = bombOutRisk(
      {
        squat: summary(0.99, 0.008),
        bench: summary(0.995, 0),
        deadlift: summary(0.998, 0),
      },
      0.017,
    );
    expect(risk.hint).toBe(
      "Squat is the riskiest lift: it bombs out in 1.0% of meets, against 1.7% for the meet as a whole. Most of it is technical misses, so cleaner technique helps more than lighter weights.",
    );
  });

  it("gives no advice when the riskiest lift is under half the meet's chance", () => {
    const risk = bombOutRisk(
      {
        squat: summary(0.99, 0),
        bench: summary(0.99, 0),
        deadlift: summary(0.995, 0),
      },
      0.025,
    );
    expect(risk.hint).toBe(
      "Squat is the riskiest lift: it bombs out in 1.0% of meets, against 2.5% for the meet as a whole.",
    );
    expect(risk.scaleMax).toBe(0.01);
  });

  it("only says bomb-outs are rare when they are", () => {
    const risk = bombOutRisk(
      {
        squat: summary(1, 0),
        bench: summary(0.996, 0),
        deadlift: summary(1, 0),
      },
      0.004,
    );
    expect(risk.hint).toBe("Bomb-outs are rare with this plan: 0.4% of meets.");
  });

  it("says so when no lift bombs out", () => {
    const risk = bombOutRisk(
      {
        squat: summary(1, 0),
        bench: summary(1, 0),
        deadlift: summary(1, 0),
      },
      0,
    );
    expect(risk.hint).toBe("No lift bombed out in the simulated meets.");
    expect(risk.overlap).toBe("");
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

describe("a path's weights", () => {
  it("follows the jumps after each make and miss", () => {
    expect(pathWeights(plan, [true, true, true])).toEqual([220, 232.5, 240]);
    expect(pathWeights(plan, [true, false, true])).toEqual([220, 232.5, 232.5]);
    expect(pathWeights(plan, [false])).toEqual([220]);
    expect(pathWeights(undefined, [true, true, true])).toEqual([]);
  });

  it("reads the weights, or the best lift while the plan is unknown", () => {
    const row = { made: [true, true, false], bestKg: 232.5, share: 0.2 };
    expect(pathText(row, plan)).toBe("220 → 232.5 → 240 kg");
    expect(pathText(row)).toBe("best 232.5 kg");
    expect(pathText({ made: [false, false, false], share: 0.1 })).toBe(
      "no lift",
    );
  });
});

it("confirms an applied change, then that the results are in", () => {
  expect(appliedLine("Squat opener: 215 kg instead of 220 kg", false)).toBe(
    "Applied: Squat opener: 215 kg instead of 220 kg. Simulating…",
  );
  expect(appliedLine("Squat opener: 215 kg instead of 220 kg", true)).toBe(
    "Applied: Squat opener: 215 kg instead of 220 kg. Results updated.",
  );
});

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
  improves = false,
): Change => ({ lift, setting, weightKg, result, improves });

describe("what-ifs", () => {
  it("describes a jump and an opener", () => {
    const current = outcome(0.7, 0.04);
    expect(
      describeChange(change("bench", "thirdAfterMake", 5, current), current),
    ).toBe("Bench press 3rd after a make: +5 kg instead of +7.5 kg");
    expect(
      describeChange(change("deadlift", "opener", 215, current), current),
    ).toBe("Deadlift opener: 215 kg instead of 220 kg");
    expect(
      describeChange(change("squat", "secondAfterMiss", 2.5, current), current),
    ).toBe("Squat 2nd after a miss: +2.5 kg instead of repeat the weight");
    expect(
      describeChange(change("squat", "secondAfterMake", 0, current), current),
    ).toBe("Squat 2nd after a make: repeat the weight instead of +12.5 kg");
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

  it("takes the changes the engine ranks higher first, then the safest", () => {
    const current = outcome(0.7, 0.04);
    const picks = whatIfs(current, [
      change("bench", "thirdAfterMake", 5, outcome(0.72, 0.04), true),
      change("squat", "opener", 222.5, outcome(0.69, 0.05)),
      change("deadlift", "opener", 215, outcome(0.6, 0.031)),
      change("squat", "opener", 217.5, outcome(0.65, 0.035)),
    ]);
    expect(picks.map((pick) => pick.text)).toEqual([
      "Bench press 3rd after a make: +5 kg instead of +7.5 kg",
      "Deadlift opener: 215 kg instead of 220 kg",
      "Squat opener: 217.5 kg instead of 220 kg",
    ]);
    expect(picks[0]!.goal).toEqual({ text: "goal +2.0%", direction: "better" });
    expect(picks[1]!.bombOut).toEqual({
      text: "bomb-out −0.9%",
      direction: "better",
    });
  });

  it("leaves out a change that neither ranks higher nor cuts the bomb-out chance", () => {
    const current = outcome(0.7, 0.05);
    const picks = whatIfs(current, [
      change("squat", "opener", 222.5, outcome(0.8, 0.06)),
      change("squat", "opener", 217.5, outcome(0.6, 0.03), true),
    ]);
    expect(picks.map((pick) => pick.text)).toEqual([
      "Squat opener: 217.5 kg instead of 220 kg",
    ]);
  });

  it("leaves out the goal delta without a goal", () => {
    const current: PlanOutcome = {
      plans,
      bombOutProbability: 0.04,
      expectedTotalKg: 540,
    };
    const [pick] = whatIfs(current, [
      change(
        "squat",
        "opener",
        222.5,
        { ...current, expectedTotalKg: 541 },
        true,
      ),
    ]);
    expect(pick!.goal).toBeUndefined();
    expect(pick!.plans).toBe(plans);
  });
});
