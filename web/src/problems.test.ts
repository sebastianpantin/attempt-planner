import { describe, expect, it } from "vitest";
import type { Problem } from "../pkg/attempt_sim_wasm.js";
import { EXAMPLE } from "./example.ts";
import {
  describeProblem,
  inputsFor,
  FIRST_MEET_CAP_PERCENT,
  firstMeetPrompt,
  maxesPrompt,
  problemListId,
  shownProblems,
} from "./problems.ts";

const problem = (
  field: Problem["field"],
  lift?: Problem["lift"],
  code: Problem["code"] = "invalidWeight",
): Problem => ({ field, code, message: "", ...(lift && { lift }) });

describe("inputsFor", () => {
  it("marks the inputs of the lift and plan step", () => {
    expect(inputsFor(problem("opener", "squat"), EXAMPLE)).toEqual([
      "squat-openerKg",
    ]);
    expect(
      inputsFor(problem("curve", "bench", "invalidCurve"), EXAMPLE),
    ).toEqual([
      "bench-ratioAtHalf",
      "bench-steepness",
      "bench-technicalMissPercent",
    ]);
  });

  it("marks only the jump at fault", () => {
    expect(
      inputsFor(problem("second", "squat", "lighter"), {
        ...EXAMPLE,
        "squat-secondAfterMissKg": -2.5,
      }),
    ).toEqual(["squat-secondAfterMissKg"]);
    expect(
      inputsFor(problem("third", "bench", "noIncreaseAfterMake"), {
        ...EXAMPLE,
        "bench-thirdAfterMakeKg": 0,
      }),
    ).toEqual(["bench-thirdAfterMakeKg"]);
    expect(
      inputsFor(problem("third", "bench"), {
        ...EXAMPLE,
        "bench-thirdAfterMissKg": Number.NaN,
      }),
    ).toEqual(["bench-thirdAfterMissKg"]);
  });

  it("marks only the day-form input at fault, or both spreads when only their sum is too big", () => {
    const dayForm = problem("dayForm", undefined, "invalidDayForm");
    expect(inputsFor(dayForm, { ...EXAMPLE, correlation: 1.5 })).toEqual([
      "correlation",
    ]);
    expect(
      inputsFor(dayForm, {
        ...EXAMPLE,
        spreadPercent: 40,
        maxUncertaintyPercent: 40,
      }),
    ).toEqual(["spreadPercent", "maxUncertaintyPercent", "correlation"]);
  });

  it("marks meet-wide inputs", () => {
    expect(inputsFor(problem("goal"), { ...EXAMPLE, goalKg: -1 })).toEqual([
      "goalKg",
    ]);
  });
});

describe("describeProblem", () => {
  it("names the lift and the field, with advice on a valid value", () => {
    expect(describeProblem(problem("expectedMax", "squat"))).toBe(
      "Squat max: enter a weight in kg.",
    );
    expect(
      describeProblem(problem("expectedMax", "bench", "expectedMaxBelowBar")),
    ).toBe("Bench press max: enter at least 20 kg, the weight of the bar.");
    expect(describeProblem(problem("second", "deadlift", "notLoadable"))).toBe(
      "Deadlift 2nd attempt: use a multiple of 2.5 kg.",
    );
  });

  it("explains an empty goal and the bomb-out limit in the page's units", () => {
    expect(describeProblem(problem("goal"))).toBe(
      "Goal total: enter a weight in kg, or leave it empty for no goal.",
    );
    expect(
      describeProblem(problem("bombOutCap", undefined, "invalidBombOutCap")),
    ).toBe("Bomb-out limit: must be from 0 to 100%.");
  });

  it("falls back on the engine's message", () => {
    expect(
      describeProblem({
        field: "simulations",
        code: "simulationsOutOfRange",
        message: "simulations must be between 1 and 10",
      }),
    ).toBe("Simulations: simulations must be between 1 and 10.");
  });
});

describe("problemListId", () => {
  it("lists a problem next to its inputs", () => {
    expect(problemListId(problem("expectedMax", "bench"))).toBe(
      "bench-max-problems",
    );
    expect(problemListId(problem("opener", "bench"))).toBe("bench-problems");
    expect(problemListId(problem("curve", "bench"))).toBe("curve-problems");
    expect(problemListId(problem("dayForm"))).toBe("day-form-problems");
    expect(problemListId(problem("goal"))).toBe("meet-problems");
  });
});

describe("waiting for the maxes", () => {
  const noBench = { ...EXAMPLE, "bench-expectedMaxKg": Number.NaN };

  it("asks for the maxes still empty", () => {
    expect(maxesPrompt(EXAMPLE)).toBe("");
    expect(maxesPrompt(noBench)).toBe(
      "Enter your bench press max to see results.",
    );
    expect(maxesPrompt({ ...noBench, "squat-expectedMaxKg": Number.NaN })).toBe(
      "Enter your squat and bench press maxes to see results.",
    );
  });

  it("names the action the maxes are wanted for", () => {
    expect(maxesPrompt(noBench, "plan your attempts")).toBe(
      "Enter your bench press max to plan your attempts.",
    );
  });

  it("tells a first-timer the limit is set and which maxes the search waits for", () => {
    expect(firstMeetPrompt(EXAMPLE)).toBe("");
    expect(firstMeetPrompt(noBench)).toBe(
      `Bomb-out limit set to ${FIRST_MEET_CAP_PERCENT}%. Enter your bench press max to plan your attempts.`,
    );
  });

  it("holds back the problems of a lift without a max, but not the rest", () => {
    const problems = [
      problem("expectedMax", "bench"),
      problem("opener", "bench"),
      problem("curve", "bench", "invalidCurve"),
      problem("opener", "squat"),
    ];
    expect(shownProblems(problems, noBench)).toEqual([
      problem("curve", "bench", "invalidCurve"),
      problem("opener", "squat"),
    ]);
  });
});
