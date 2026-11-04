import { describe, expect, it } from "vitest";
import type { Problem } from "../pkg/attempt_sim_wasm.js";
import { EXAMPLE } from "./example.ts";
import {
  buildRequest,
  buildSearchRequest,
  buildTradeoffRequest,
  planValues,
  describeProblem,
  INPUT_NAMES,
  inputsFor,
  PRECISE_SIMULATIONS,
  QUICK_SIMULATIONS,
} from "./inputs.ts";

describe("buildRequest", () => {
  it("reads each lift's model and plan from its named inputs", () => {
    const request = buildRequest(
      {
        ...EXAMPLE,
        "bench-expectedMaxKg": 135,
        "bench-openerKg": 122.5,
        "bench-secondAfterMakeKg": 7.5,
        "bench-secondAfterMissKg": 2.5,
        "bench-thirdAfterMakeKg": 5,
        "bench-thirdAfterMissKg": 0,
        "bench-ratioAtHalf": 1.02,
        "bench-steepness": 40,
        "bench-technicalMissPercent": 2.5,
      },
      false,
    );
    expect(request.lifter.lifts.bench).toEqual({
      expectedMaxKg: 135,
      curve: { ratioAtHalf: 1.02, steepness: 40, technicalMissChance: 0.025 },
    });
    expect(request.plans.bench).toEqual({
      openerKg: 122.5,
      second: { afterMakeKg: 7.5, afterMissKg: 2.5 },
      third: { afterMakeKg: 5, afterMissKg: 0 },
    });
  });

  it("takes the day-form spreads in percent", () => {
    const request = buildRequest(
      {
        ...EXAMPLE,
        spreadPercent: 4,
        maxUncertaintyPercent: 2,
        correlation: 0.3,
      },
      false,
    );
    expect(request.lifter.dayForm).toEqual({
      spread: 0.04,
      maxUncertainty: 0.02,
      correlation: 0.3,
    });
  });

  it("leaves the goal out when it is empty", () => {
    expect(buildRequest({ ...EXAMPLE, goalKg: 560 }, false).goalKg).toBe(560);
    expect(buildRequest({ ...EXAMPLE, goalKg: NaN }, false)).not.toHaveProperty(
      "goalKg",
    );
  });

  it("runs more meets in precise mode", () => {
    expect(buildRequest(EXAMPLE, false).simulations).toBe(QUICK_SIMULATIONS);
    expect(buildRequest(EXAMPLE, true).simulations).toBe(PRECISE_SIMULATIONS);
  });

  it("uses the same seed every time", () => {
    expect(buildRequest(EXAMPLE, true).seed).toBe(
      buildRequest(EXAMPLE, false).seed,
    );
  });
});

describe("inputsFor", () => {
  const problem = (
    field: Problem["field"],
    lift?: Problem["lift"],
  ): Problem => ({
    field,
    code: "invalidWeight",
    message: "",
    ...(lift && { lift }),
  });

  it("marks the inputs of the lift and plan step", () => {
    expect(inputsFor(problem("opener", "squat"))).toEqual(["squat-openerKg"]);
    expect(inputsFor(problem("third", "deadlift"))).toEqual([
      "deadlift-thirdAfterMakeKg",
      "deadlift-thirdAfterMissKg",
    ]);
    expect(inputsFor(problem("curve", "bench"))).toEqual([
      "bench-ratioAtHalf",
      "bench-steepness",
      "bench-technicalMissPercent",
    ]);
  });

  it("marks meet-wide inputs", () => {
    expect(inputsFor(problem("goal"))).toEqual(["goalKg"]);
    expect(inputsFor(problem("dayForm"))).toEqual([
      "spreadPercent",
      "maxUncertaintyPercent",
      "correlation",
    ]);
  });
});

describe("describeProblem", () => {
  it("names the input before the engine's message", () => {
    expect(
      describeProblem({
        lift: "squat",
        field: "second",
        code: "notLoadable",
        message: "attempt 2 is not a multiple of 2.5 kg",
      }),
    ).toBe("2nd attempt: attempt 2 is not a multiple of 2.5 kg");
  });

  it("states caps in the page's percent units", () => {
    expect(
      describeProblem({
        field: "bombOutCap",
        code: "invalidBombOutCap",
        message: "bomb-out cap must be a chance between 0 and 1",
      }),
    ).toBe("Bomb-out cap: must be from 0 to 100%");
  });
});

it("lists every number input once", () => {
  expect(INPUT_NAMES).toHaveLength(3 * 9 + 5);
  expect(new Set(INPUT_NAMES).size).toBe(INPUT_NAMES.length);
  expect(Object.keys(EXAMPLE).sort()).toEqual([...INPUT_NAMES].sort());
});

describe("buildTradeoffRequest", () => {
  it("takes the lifter, goal and cap but no plans, always at the quick count", () => {
    const evaluate = buildRequest(EXAMPLE, true);
    const sweep = buildTradeoffRequest({ ...EXAMPLE, bombOutCapPercent: 5 });
    expect(sweep).toEqual({
      lifter: evaluate.lifter,
      goalKg: evaluate.goalKg,
      bombOutCap: 0.05,
      simulations: QUICK_SIMULATIONS,
      seed: evaluate.seed,
    });
  });

  it("leaves the cap out when it is empty, and out of evaluations", () => {
    expect(
      buildTradeoffRequest({ ...EXAMPLE, bombOutCapPercent: NaN }),
    ).not.toHaveProperty("bombOutCap");
    expect(buildRequest(EXAMPLE, false)).not.toHaveProperty("bombOutCap");
  });
});

describe("planValues", () => {
  it("fills the plan inputs so the request carries the same plans", () => {
    const plans = buildRequest(
      { ...EXAMPLE, "deadlift-openerKg": 210, "bench-thirdAfterMissKg": 2.5 },
      false,
    ).plans;
    const values = { ...EXAMPLE, ...Object.fromEntries(planValues(plans)) };
    expect(buildRequest(values, false).plans).toEqual(plans);
    expect(values["deadlift-openerKg"]).toBe(210);
  });
});

describe("buildSearchRequest", () => {
  it("takes the lifter, plans and goal, with the cap in percent", () => {
    const evaluate = buildRequest(EXAMPLE, false);
    expect(buildSearchRequest({ ...EXAMPLE, bombOutCapPercent: 2.5 })).toEqual({
      lifter: evaluate.lifter,
      plans: evaluate.plans,
      goalKg: evaluate.goalKg,
      bombOutCap: 0.025,
      simulations: QUICK_SIMULATIONS,
      seed: evaluate.seed,
    });
  });

  it("leaves the cap out when it is empty", () => {
    expect(
      buildSearchRequest({ ...EXAMPLE, bombOutCapPercent: NaN }),
    ).not.toHaveProperty("bombOutCap");
  });
});
