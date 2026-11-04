import { expect, it } from "vitest";
import { finePercent, kg, percent, simulationError } from "./format.ts";

it("shows kg to one decimal, dropping a trailing zero", () => {
  expect(kg(200)).toBe("200 kg");
  expect(kg(202.5)).toBe("202.5 kg");
  expect(kg(512.3456)).toBe("512.3 kg");
});

it("shows probabilities with exactly one decimal", () => {
  expect(percent(0.0512)).toBe("5.1%");
  expect(percent(0.98)).toBe("98.0%");
  expect(percent(0.04)).toBe("4.0%");
  expect(percent(1)).toBe("100.0%");
});

it("shows probabilities under 1% with two decimals", () => {
  expect(percent(0.0014)).toBe("0.14%");
  expect(percent(0.00012)).toBe("0.01%");
  expect(percent(0)).toBe("0.00%");
});

it("drops trailing zeros for axis ticks", () => {
  expect(finePercent(0.005)).toBe("0.5%");
  expect(finePercent(0.01)).toBe("1%");
  expect(finePercent(0.00125)).toBe("0.13%");
});

it("sums up the simulation error in one line", () => {
  expect(
    simulationError(
      {
        expectedTotalKg: 0.83,
        goalProbability: 0.0032,
        bombOutProbability: 0.0014,
      },
      true,
    ),
  ).toBe(
    "Simulation error: about ±0.8 kg on the total, ±0.32% on the goal chance and ±0.14% on the bomb-out chance. Tick Precise to narrow it.",
  );
  expect(
    simulationError(
      { expectedTotalKg: 0.1, bombOutProbability: 0.0002 },
      false,
    ),
  ).toBe(
    "Simulation error: about ±0.1 kg on the total and ±0.02% on the bomb-out chance.",
  );
});
