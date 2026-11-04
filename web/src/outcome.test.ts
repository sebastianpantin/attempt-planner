import { describe, expect, it } from "vitest";
import {
  averageLine,
  barFigures,
  chanceInWords,
  outcomeSentence,
  rangeLabels,
  rangeLayout,
  ringFigure,
  riskWarning,
} from "./outcome.ts";

const figures = {
  expectedTotalKg: 535.24,
  goalProbability: 0.7 as number | undefined,
  bombOutProbability: 0.04,
  totalPercentilesKg: { p10: 532.5, p50: 557.5, p90: 580 },
};

const text = (phrases: { text: string }[]) =>
  phrases.map((phrase) => phrase.text).join("");

describe("chanceInWords", () => {
  it("counts in tenths in the middle", () => {
    expect(chanceInWords(0.7)).toBe("7 of 10");
    expect(chanceInWords(0.1)).toBe("1 of 10");
    expect(chanceInWords(0.9)).toBe("9 of 10");
  });

  it("names a rare chance as one in so many, rounded to a friendly count", () => {
    expect(chanceInWords(0.04)).toBe("1 in 25");
    expect(chanceInWords(0.155)).toBe("2 of 10");
    expect(chanceInWords(0.0135)).toBe("1 in 75");
    expect(chanceInWords(0.004)).toBe("1 in 250");
    expect(chanceInWords(1 / 741)).toBe("fewer than 1 in 500");
    expect(chanceInWords(0.0004)).toBe("fewer than 1 in 500");
  });

  it("names a near-certain chance as all but one", () => {
    expect(chanceInWords(0.96)).toBe("24 of 25");
    expect(chanceInWords(0.9996)).toBe("almost all");
  });
});

describe("outcomeSentence", () => {
  it("tells how often the goal is reached and the meet bombed out", () => {
    const sentence = outcomeSentence(figures, 550);
    expect(text(sentence)).toBe(
      "7 of 10 simulated meets reach 550 kg or more. 1 in 25 simulated meets end in a bomb-out.",
    );
    expect(
      sentence.filter((phrase) => phrase.strong).map(({ text }) => text),
    ).toEqual(["7 of 10", "1 in 25"]);
  });

  it("gives the typical total without a goal, which bomb-outs do not drag down", () => {
    expect(text(outcomeSentence(figures, undefined))).toBe(
      "Half the simulated meets total 557.5 kg or more. 1 in 25 simulated meets end in a bomb-out.",
    );
    expect(
      text(
        outcomeSentence({
          ...figures,
          bombOutProbability: 0.0004,
          totalPercentilesKg: { p10: 0, p50: 0, p90: 600 },
        }),
      ),
    ).toBe(
      "Half the simulated meets or more end in a bomb-out. Fewer than 1 in 500 simulated meets end in a bomb-out.",
    );
  });

  it("speaks of none and every rather than a count", () => {
    expect(
      text(
        outcomeSentence(
          { ...figures, goalProbability: 0, bombOutProbability: 0 },
          600,
        ),
      ),
    ).toBe("No simulated meet reaches 600 kg. No simulated meet bombs out.");
    expect(
      text(outcomeSentence({ ...figures, goalProbability: 1 }, 400)),
    ).toMatch(/^Every simulated meet reaches 400 kg or more\./);
  });
});

describe("ringFigure", () => {
  it("shows the goal chance with a goal", () => {
    expect(ringFigure(figures, 550)).toEqual({
      value: "70.0%",
      label: "reach 550 kg",
      share: 0.7,
    });
  });

  it("shows the typical total, filled by the meets without a bomb-out, without a goal", () => {
    expect(ringFigure({ ...figures, goalProbability: undefined })).toEqual({
      value: "557.5 kg",
      label: "typical total",
      share: 0.96,
    });
  });
});

describe("barFigures", () => {
  it("leads with the goal chance, then the bomb-out chance", () => {
    expect(barFigures(figures, 550)).toEqual([
      ["70.0%", "reach 550 kg"],
      ["4.0%", "bomb-out"],
    ]);
  });

  it("leads with the typical total without a goal", () => {
    expect(barFigures({ ...figures, goalProbability: undefined })).toEqual([
      ["557.5 kg", "typical total"],
      ["4.0%", "bomb-out"],
    ]);
  });
});

describe("rangeLayout", () => {
  const totals = { p10: 530, p50: 555, p90: 580 };

  it("places the likely range, the median and the goal on one scale", () => {
    const layout = rangeLayout(totals, 550);
    expect(layout.fromPercent).toBeCloseTo(16.67);
    expect(layout.toPercent).toBeCloseTo(83.33);
    expect(layout.medianPercent).toBeCloseTo(50);
    expect(layout.goalPercent).toBeCloseTo(43.33);
  });

  it("widens the scale to take in a goal outside the range", () => {
    const layout = rangeLayout(totals, 600);
    expect(layout.goalPercent).toBeLessThan(100);
    expect(layout.toPercent).toBeLessThan(layout.goalPercent!);
  });

  it("leaves out the goal without one and copes with a single total", () => {
    expect(rangeLayout(totals).goalPercent).toBeUndefined();
    const single = rangeLayout({ p10: 500, p50: 500, p90: 500 });
    expect(single.fromPercent).toBe(50);
    expect(single.toPercent).toBe(50);
  });
});

describe("rangeLayout with bomb-outs", () => {
  it("puts a bomb-out at the left edge and scales to the totals posted", () => {
    const layout = rangeLayout({ p10: 0, p50: 540, p90: 580 });
    expect(layout.fromPercent).toBe(0);
    expect(layout.medianPercent).toBeCloseTo(16.67);
    expect(layout.toPercent).toBeCloseTo(83.33);
  });

  it("copes with every likely total a bomb-out", () => {
    expect(rangeLayout({ p10: 0, p50: 0, p90: 0 })).toEqual({
      fromPercent: 0,
      toPercent: 0,
      medianPercent: 0,
      goalPercent: undefined,
    });
  });
});

describe("rangeLabels", () => {
  it("drops a needless .0 and adds no note without bomb-outs in the range", () => {
    expect(rangeLabels({ p10: 530, p50: 555, p90: 580 })).toEqual({
      low: "530 kg",
      median: "median 555",
      high: "580 kg",
      note: "",
    });
  });

  it("names a bomb-out instead of 0 kg", () => {
    expect(rangeLabels({ p10: 0, p50: 540, p90: 580 })).toEqual({
      low: "bomb-out",
      median: "median 540",
      high: "580 kg",
      note: "1 in 10 meets or more ends in a bomb-out.",
    });
    expect(rangeLabels({ p10: 0, p50: 0, p90: 0 })).toEqual({
      low: "bomb-out",
      median: "median bomb-out",
      high: "bomb-out",
      note: "Half the meets or more end in a bomb-out.",
    });
  });
});

describe("averageLine", () => {
  it("says that a bomb-out counts as 0 kg, and gives the average with a total", () => {
    expect(averageLine(534.8, 557.1)).toBe(
      "Average total: 534.8 kg counting a bomb-out as 0 kg; 557.1 kg in the meets where you post a total.",
    );
    expect(averageLine(0, undefined)).toBe(
      "Average total: 0 kg counting a bomb-out as 0 kg; every meet bombed out.",
    );
  });
});

describe("riskWarning", () => {
  it("warns only above a 10% bomb-out chance", () => {
    expect(riskWarning(0.1)).toBe("");
    expect(riskWarning(0.155)).toMatch(/^High risk: 15\.5% of meets end/);
  });
});
