import { expect, it } from "vitest";
import {
  finePercent,
  kg,
  kgWithError,
  percent,
  percentWithError,
} from "./format.ts";

it("shows kg to one decimal, dropping a trailing zero", () => {
  expect(kg(200)).toBe("200 kg");
  expect(kg(202.5)).toBe("202.5 kg");
  expect(kg(512.3456)).toBe("512.3 kg");
});

it("shows probabilities as percentages", () => {
  expect(percent(0.0512)).toBe("5.1%");
  expect(percent(1)).toBe("100%");
});

it("shows small probabilities to two decimals when asked", () => {
  expect(finePercent(0.00012)).toBe("0.01%");
});

it("adds the standard error", () => {
  expect(kgWithError(512.34, 0.43)).toBe("512.3 kg ± 0.4 kg");
  expect(percentWithError(0.0512, 0.00157)).toBe("5.1% ± 0.16%");
});
