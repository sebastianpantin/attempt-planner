import { describe, expect, it } from "vitest";
import {
  aboveMaxWarnings,
  attemptsAboveMax,
  defaultShape,
  defaultShapes,
  isPlanField,
  regroupedShapes,
  scaledPlan,
  shapeOf,
} from "./attempt-scaling.ts";
import { CALIBRATIONS, EXAMPLE } from "./example.ts";

const plan = {
  openerKg: 182.5,
  second: { afterMakeKg: 12.5, afterMissKg: 0 },
  third: { afterMakeKg: 7.5, afterMissKg: 0 },
};

describe("shapeOf and scaledPlan", () => {
  it("rescale a plan proportionally to a new max, on 2.5 kg steps", () => {
    const shape = shapeOf(EXAMPLE, "squat")!;
    expect(scaledPlan(shape, "squat", 140)).toEqual([
      ["squat-openerKg", 127.5],
      ["squat-secondAfterMakeKg", 10],
      ["squat-secondAfterMissKg", 0],
      ["squat-thirdAfterMakeKg", 5],
      ["squat-thirdAfterMissKg", 0],
    ]);
  });

  it("give back the same plan at the same max", () => {
    const shape = shapeOf(EXAMPLE, "bench")!;
    expect(new Map(scaledPlan(shape, "bench", 130))).toEqual(
      new Map([
        ["bench-openerKg", 120],
        ["bench-secondAfterMakeKg", 7.5],
        ["bench-secondAfterMissKg", 0],
        ["bench-thirdAfterMakeKg", 5],
        ["bench-thirdAfterMissKg", 0],
      ]),
    );
  });

  it("keep the plan valid at a tiny max: the opener at the bar and a jump after a make", () => {
    const shape = shapeOf(EXAMPLE, "squat")!;
    expect(new Map(scaledPlan(shape, "squat", 20))).toEqual(
      new Map([
        ["squat-openerKg", 20],
        ["squat-secondAfterMakeKg", 2.5],
        ["squat-secondAfterMissKg", 0],
        ["squat-thirdAfterMakeKg", 2.5],
        ["squat-thirdAfterMissKg", 0],
      ]),
    );
  });

  it("have no shape while the max or a weight is missing or below the bar", () => {
    expect(
      shapeOf({ ...EXAMPLE, "squat-expectedMaxKg": Number.NaN }, "squat"),
    ).toBeUndefined();
    expect(
      shapeOf({ ...EXAMPLE, "squat-expectedMaxKg": 10 }, "squat"),
    ).toBeUndefined();
    expect(
      shapeOf({ ...EXAMPLE, "squat-openerKg": Number.NaN }, "squat"),
    ).toBeUndefined();
  });

  it("start a newcomer near the typical opener and finish at or under the max", () => {
    expect(new Map(scaledPlan(defaultShape(0.89), "squat", 140))).toEqual(
      new Map([
        ["squat-openerKg", 125],
        ["squat-secondAfterMakeKg", 7.5],
        ["squat-secondAfterMissKg", 0],
        ["squat-thirdAfterMakeKg", 5],
        ["squat-thirdAfterMissKg", 0],
      ]),
    );
  });
});

describe("default shapes", () => {
  const { any, women } = CALIBRATIONS;

  it("start each lift at its group's typical opener", () => {
    const shapes = defaultShapes(women.typicalOpener);
    expect(shapes.squat).toEqual(defaultShape(0.88));
    expect(shapes.deadlift).toEqual(defaultShape(0.87));
  });

  it("follow a group change only where the shape is still the old group's default", () => {
    const shapes = {
      ...defaultShapes(any.typicalOpener),
      bench: shapeOf(EXAMPLE, "bench")!,
    };
    expect(
      regroupedShapes(shapes, any.typicalOpener, women.typicalOpener),
    ).toEqual([
      ["squat", defaultShape(0.88)],
      ["deadlift", defaultShape(0.87)],
    ]);
  });
});

describe("isPlanField", () => {
  it("tells the opener and jumps from the model inputs", () => {
    expect(isPlanField("openerKg")).toBe(true);
    expect(isPlanField("thirdAfterMissKg")).toBe(true);
    expect(isPlanField("expectedMaxKg")).toBe(false);
    expect(isPlanField("steepness")).toBe(false);
  });
});

describe("attemptsAboveMax", () => {
  it("lists each attempt whose heaviest weight is above the max", () => {
    expect(attemptsAboveMax(plan, 200)).toEqual([
      { attempt: "3rd attempt", kg: 202.5 },
    ]);
    expect(attemptsAboveMax(plan, 140).map(({ attempt }) => attempt)).toEqual([
      "Opener",
      "2nd attempt",
      "3rd attempt",
    ]);
    expect(attemptsAboveMax(plan, 210)).toEqual([]);
  });

  it("counts the heavier path after a miss", () => {
    const missUp = { ...plan, second: { afterMakeKg: 2.5, afterMissKg: 10 } };
    expect(attemptsAboveMax(missUp, 190)).toEqual([
      { attempt: "2nd attempt", kg: 192.5 },
      { attempt: "3rd attempt", kg: 200 },
    ]);
  });

  it("says nothing while the max or a weight is missing", () => {
    expect(attemptsAboveMax(plan, Number.NaN)).toEqual([]);
    expect(attemptsAboveMax({ ...plan, openerKg: Number.NaN }, 100)).toEqual(
      [],
    );
  });
});

describe("aboveMaxWarnings", () => {
  it("warns most about an opener above the max", () => {
    expect(aboveMaxWarnings(plan, 180)).toEqual([
      "Opener 182.5 kg is above your 180 kg max, so you'd miss it more often than make it. Lower it, or press Find my best plan.",
      "2nd attempt 195 kg is above your 180 kg max, so it's made less than half the time.",
      "3rd attempt 202.5 kg is above your 180 kg max, so it's made less than half the time.",
    ]);
  });
});
