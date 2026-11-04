/** Short explanations of the words the page uses, each shown by a "?" toggle. */
export const TERMS = {
  expectedMax: {
    name: "an expected max",
    text: "The weight you'd make about half the time on a normal meet day, with judged depth, commands and a paused bench. It is not your best gym PR. Never competed? Use a recent gym max done to meet standards, or a few kg under it.",
  },
  opener: {
    name: "an opener",
    text: "Your first attempt. Pick a weight you'd make even on a bad day, so you get a total on the board.",
  },
  bombOut: {
    name: "a bomb-out",
    text: "Missing all three attempts on one lift. You get no total, so the meet ends without a result.",
  },
  technicalMiss: {
    name: "a technical miss",
    text: "A miss at any weight, for example on depth, a missed command or the bar moving down. It is a fixed chance per attempt.",
  },
  percentile: {
    name: "a percentile",
    text: "The 10th percentile is the total that 1 in 10 simulated meets fall below, a bad day. The median, the 50th, is the typical total. The 90th is a good day.",
  },
  dayForm: {
    name: "day form",
    text: "How strong you are on the day. Your max on the day varies around the expected max, so the same plan can go well or badly.",
  },
} as const;

export type Term = keyof typeof TERMS;

export const isTerm = (key: string): key is Term => key in TERMS;

/** A toggle that shows a term's explanation by click, tap or keyboard. */
export function termHelp(term: Term): string {
  const { name, text } = TERMS[term];
  return `<details class="term"><summary><span aria-hidden="true">?</span><span class="visually-hidden">What is ${name}?</span></summary><p>${text}</p></details>`;
}
