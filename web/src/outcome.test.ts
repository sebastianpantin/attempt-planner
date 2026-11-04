import { describe, expect, it } from "vitest";
import {
  barFigures,
  chanceInWords,
  outcomeSentence,
  rangeLayout,
  ringFigure,
} from "./outcome.ts";

const figures = {
  expectedTotalKg: 535.24,
  goalProbability: 0.7 as number | undefined,
  bombOutProbability: 0.04,
};

const text = (phrases: { text: string }[]) =>
  phrases.map((phrase) => phrase.text).join("");

describe("chanceInWords", () => {
  it("counts in tenths in the middle", () => {
    expect(chanceInWords(0.7)).toBe("7 of 10");
    expect(chanceInWords(0.1)).toBe("1 of 10");
    expect(chanceInWords(0.9)).toBe("9 of 10");
  });

  it("names a rare chance as one in so many", () => {
    expect(chanceInWords(0.04)).toBe("1 of 25");
    expect(chanceInWords(0.0004)).toBe("fewer than 1 of 1,000");
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
      "In 7 of 10 simulated meets you total 550 kg or more. You bomb out in 1 of 25.",
    );
    expect(
      sentence.filter((phrase) => phrase.strong).map(({ text }) => text),
    ).toEqual(["7 of 10", "1 of 25"]);
  });

  it("gives the average total without a goal", () => {
    expect(text(outcomeSentence(figures, undefined))).toBe(
      "On average you total 535.2 kg, counting a bomb-out as 0. You bomb out in 1 of 25.",
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

  it("shows the average total, filled by the meets without a bomb-out, without a goal", () => {
    expect(ringFigure({ ...figures, goalProbability: undefined })).toEqual({
      value: "535.2 kg",
      label: "average total",
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

  it("leads with the average total without a goal", () => {
    expect(barFigures({ ...figures, goalProbability: undefined })).toEqual([
      ["535.2 kg", "average"],
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
