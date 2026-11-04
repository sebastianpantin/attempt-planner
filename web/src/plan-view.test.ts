import { describe, expect, it } from "vitest";
import {
  bombOutCapLabel,
  bombOutCapPercent,
  ladder,
  problemListId,
} from "./plan-view.ts";

const plan = {
  openerKg: 182.5,
  second: { afterMakeKg: 12.5, afterMissKg: 0 },
  third: { afterMakeKg: 7.5, afterMissKg: 0 },
};

describe("ladder", () => {
  it("gives the weights if every attempt is made, the opener's share of the max and the main paths", () => {
    expect(ladder(plan, 200)).toEqual({
      secondKg: 195,
      thirdKg: 202.5,
      shareOfMax: "91% of your max",
      line: "182.5 → 195 → 202.5",
      paths: [
        ["All made", "182.5 → 195 → 202.5"],
        ["Miss the opener", "182.5 → 182.5 → 190"],
        ["Miss the 2nd", "182.5 → 195 → 195"],
      ],
    });
  });

  it("leaves out what a missing weight or max makes unknown", () => {
    expect(ladder({ ...plan, openerKg: Number.NaN }, Number.NaN)).toEqual({
      paths: [],
    });
    expect(ladder(plan, Number.NaN).shareOfMax).toBeUndefined();
  });
});

describe("bombOutCapPercent", () => {
  it("reads the slider, or no cap when there is no limit", () => {
    expect(bombOutCapPercent(5, false)).toBe(5);
    expect(bombOutCapPercent(5, true)).toBeNaN();
  });
});

describe("bombOutCapLabel", () => {
  it("shows the cap as a percentage, or no limit", () => {
    expect(bombOutCapLabel(5, false)).toBe("5%");
    expect(bombOutCapLabel(2.5, false)).toBe("2.5%");
    expect(bombOutCapLabel(5, true)).toBe("No limit");
  });
});

describe("problemListId", () => {
  it("lists a problem with an expected max under the maxes", () => {
    expect(
      problemListId({
        lift: "bench",
        field: "expectedMax",
        code: "invalidWeight",
        message: "",
      }),
    ).toBe("maxes-problems");
  });

  it("lists other problems under their lift, or the meet", () => {
    expect(
      problemListId({
        lift: "bench",
        field: "opener",
        code: "invalidWeight",
        message: "",
      }),
    ).toBe("bench-problems");
    expect(
      problemListId({ field: "goal", code: "invalidWeight", message: "" }),
    ).toBe("meet-problems");
  });
});
