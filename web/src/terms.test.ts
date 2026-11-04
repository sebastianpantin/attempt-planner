import { expect, it } from "vitest";
import { isTerm, termHelp, TERMS } from "./terms.ts";

it("explains each term behind a toggle named as a question", () => {
  const help = termHelp("bombOut");
  expect(help).toContain("<summary>");
  expect(help).toContain("What is a bomb-out?");
  expect(help).toContain(TERMS.bombOut.text);
});

it("covers the words a newcomer meets first", () => {
  for (const term of [
    "expectedMax",
    "opener",
    "bombOut",
    "technicalMiss",
    "percentile",
    "dayForm",
  ]) {
    expect(isTerm(term)).toBe(true);
  }
  expect(isTerm("steepness")).toBe(false);
});
