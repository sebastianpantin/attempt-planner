import { expect, it } from "vitest";
import { LatestOnly } from "./latest-only.ts";

function queue() {
  const sent: number[] = [];
  return { sent, latest: new LatestOnly<number>((n) => sent.push(n)) };
}

it("sends a request straight away when idle", () => {
  const { sent, latest } = queue();
  latest.submit(1);
  expect(sent).toEqual([1]);
});

it("keeps only the newest request while one is running", () => {
  const { sent, latest } = queue();
  latest.submit(1);
  latest.submit(2);
  latest.submit(3);
  expect(sent).toEqual([1]);
  expect(latest.finish()).toBeUndefined();
  expect(sent).toEqual([1, 3]);
  expect(latest.finish()).toBe(3);
  expect(sent).toEqual([1, 3]);
});

it("is idle again after the newest request finishes", () => {
  const { sent, latest } = queue();
  latest.submit(1);
  latest.finish();
  latest.submit(2);
  expect(sent).toEqual([1, 2]);
});
