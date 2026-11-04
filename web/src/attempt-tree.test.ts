import { describe, expect, it } from "vitest";
import { attemptTree } from "./attempt-tree.ts";

const plan = {
  openerKg: 190,
  second: { afterMakeKg: 12.5, afterMissKg: 0 },
  third: { afterMakeKg: 7.5, afterMissKg: 2.5 },
};

describe("attemptTree", () => {
  it("follows each outcome to the next attempt", () => {
    expect(attemptTree(plan)).toEqual({
      weightKg: 190,
      afterMake: {
        weightKg: 202.5,
        afterMake: { weightKg: 210 },
        afterMiss: { weightKg: 205 },
      },
      afterMiss: {
        weightKg: 190,
        afterMake: { weightKg: 197.5 },
        afterMiss: { weightKg: 192.5 },
      },
    });
  });

  it("has no tree until every weight is a number", () => {
    expect(
      attemptTree({ ...plan, third: { afterMakeKg: NaN, afterMissKg: 0 } }),
    ).toBeUndefined();
  });
});
