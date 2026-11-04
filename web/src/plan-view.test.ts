import { describe, expect, it } from "vitest";
import { EXAMPLE } from "./example.ts";
import {
  BOMB_OUT_CAPS,
  bombOutCapLabel,
  bombOutCapPercent,
  jumpLabels,
  ladder,
  sliderPosition,
  valueText,
} from "./plan-view.ts";

const plan = {
  openerKg: 182.5,
  second: { afterMakeKg: 12.5, afterMissKg: 0 },
  third: { afterMakeKg: 7.5, afterMissKg: 0 },
};

describe("ladder", () => {
  it("gives the weights if every attempt is made, the opener's share of the max and the main paths", () => {
    expect(ladder(plan, 200)).toEqual({
      secondKg: "195",
      thirdKg: "202.5",
      shareOfMax: "91% of your max",
      line: "182.5 → 195 → 202.5",
      paths: [
        ["All made", "182.5 → 195 → 202.5"],
        ["Miss the opener", "182.5 → 182.5 → 190"],
        ["Miss the 2nd", "182.5 → 195 → 195"],
      ],
    });
  });

  it("shows a dash for attempts a missing weight makes unknown, and leaves out the rest", () => {
    expect(ladder({ ...plan, openerKg: Number.NaN }, Number.NaN)).toEqual({
      secondKg: "–",
      thirdKg: "–",
      shareOfMax: "",
      line: "",
      paths: [],
    });
    expect(ladder(plan, Number.NaN).shareOfMax).toBe("");
  });
});

describe("jumpLabels", () => {
  it("signs each jump, reads 0 after a miss as a repeat and shows nothing over an empty input", () => {
    expect(
      jumpLabels(
        {
          ...EXAMPLE,
          "bench-secondAfterMakeKg": 12.5,
          "bench-secondAfterMissKg": 0,
          "bench-thirdAfterMakeKg": Number.NaN,
          "bench-thirdAfterMissKg": 2.5,
        },
        "bench",
      ),
    ).toEqual([
      ["bench-secondAfterMakeKg", "+12.5"],
      ["bench-secondAfterMissKg", "repeat"],
      ["bench-thirdAfterMakeKg", undefined],
      ["bench-thirdAfterMissKg", "+2.5"],
    ]);
  });
});

describe("the bomb-out cap slider", () => {
  it("runs from 0 to 100% in half-percent steps up to 10%, coarser above", () => {
    expect(BOMB_OUT_CAPS[0]).toBe(0);
    expect(BOMB_OUT_CAPS.slice(1, 4)).toEqual([0.5, 1, 1.5]);
    expect(BOMB_OUT_CAPS.at(-1)).toBe(100);
    expect(BOMB_OUT_CAPS).toContain(10);
    expect(BOMB_OUT_CAPS).toContain(30);
    expect(
      BOMB_OUT_CAPS.every((cap, i) => i === 0 || cap > BOMB_OUT_CAPS[i - 1]!),
    ).toBe(true);
  });

  it("reads the slider's cap, or no cap when there is no limit", () => {
    expect(bombOutCapPercent(sliderPosition(5), false)).toBe(5);
    expect(bombOutCapPercent(sliderPosition(60), false)).toBe(60);
    expect(bombOutCapPercent(sliderPosition(5), true)).toBeNaN();
    expect(bombOutCapPercent(1000, false)).toBe(100);
  });

  it("puts a cap between steps on the nearest one", () => {
    expect(BOMB_OUT_CAPS[sliderPosition(61)]).toBe(60);
    expect(BOMB_OUT_CAPS[sliderPosition(2.4)]).toBe(2.5);
  });

  it("shows the cap as a percentage, or any risk", () => {
    expect(bombOutCapLabel(sliderPosition(5), false)).toBe("5%");
    expect(bombOutCapLabel(sliderPosition(2.5), false)).toBe("2.5%");
    expect(bombOutCapLabel(sliderPosition(5), true)).toBe("Any risk");
  });
});

describe("valueText", () => {
  it("reads a jump as more or less, a repeat in words and the opener as a weight", () => {
    expect(valueText("openerKg", 182.5)).toBe("182.5 kg");
    expect(valueText("secondAfterMakeKg", 12.5)).toBe("12.5 kg more");
    expect(valueText("secondAfterMissKg", 0)).toBe("repeat the same weight");
    expect(valueText("thirdAfterMissKg", -2.5)).toBe("2.5 kg less");
    expect(valueText("thirdAfterMakeKg", Number.NaN)).toBe("empty");
  });
});
