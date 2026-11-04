import type {
  ChangesResponse,
  Evaluation,
  Lift,
  LiftPlanKg,
  PerLift,
} from "../../pkg/attempt_sim_wasm.js";
import { axisBottom, axisLeft } from "d3-axis";
import { scaleLinear } from "d3-scale";
import { select } from "d3-selection";
import { curveStepBefore, line } from "d3-shape";
import { kg, percent } from "../format.ts";
import { LIFTS } from "../inputs.ts";
import {
  attemptView,
  bombOutRisk,
  chanceOfAtLeast,
  dayView,
  histogram,
  LIFT_NAMES,
  reachCurve,
  whatIfs,
  type Delta,
} from "./analytics-data.ts";
import "./analytics.css";

export interface AnalyticsData {
  evaluation: Evaluation;
  changes?: Extract<ChangesResponse, { kind: "changed" }>;
  goalKg?: number;
  /** The search's highest acceptable bomb-out chance, which decides which changes improve the plan. */
  bombOutCap?: number;
  onApply: (plans: PerLift<LiftPlanKg>) => void;
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** The lift each root shows in "How each attempt goes", kept across renders. */
const shownLift = new WeakMap<HTMLElement, Lift>();

/** Fills `root` with the plan analytics panels, replacing what was there. */
export function renderAnalytics(root: HTMLElement, data: AnalyticsData) {
  const { evaluation, goalKg } = data;
  const container = document.createElement("div");
  container.className = "analytics";
  container.append(
    panel("Where your total lands", true, [
      sub(
        "Share of simulated meets by total. Bomb-outs count as 0 and are shown on the left.",
      ),
      histogramChart(evaluation, goalKg),
      legend([
        ...(goalKg === undefined
          ? [["total", "reach"]]
          : [
              ["reaches the goal", "reach"],
              ["short of the goal", "short"],
            ]),
        ["bomb-out", "bomb"],
      ]),
    ]),
    panel("Chance of reaching any total", false, [
      sub("Chance of a total at or above each weight."),
      reachChart(evaluation, goalKg),
    ]),
    riskPanel(evaluation),
    attemptPanel(root, evaluation),
    dayPanel(evaluation),
    whatIfPanel(data),
  );
  root.replaceChildren(container);
}

function panel(title: string, wide: boolean, children: Node[]) {
  const section = document.createElement("section");
  section.className = wide ? "panel wide" : "panel";
  const heading = document.createElement("h3");
  heading.textContent = title;
  section.append(heading, ...children);
  return section;
}

function element<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className: string,
  text = "",
) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  node.textContent = text;
  return node;
}

const sub = (text: string) => element("p", "sub", text);

function legend(items: string[][]) {
  const node = element("div", "legend");
  for (const [label, swatch] of items) {
    const item = element("span", "");
    item.append(element("i", `swatch ${swatch}`), label!);
    node.append(item);
  }
  return node;
}

function chartSvg(width: number, height: number, label: string) {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", `0 0 ${width} ${height}`);
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", label);
  return select(svg);
}

function histogramChart(evaluation: Evaluation, goalKg?: number) {
  const { bombOutShare, bins } = histogram(evaluation.totals, goalKg);
  const goalChance =
    goalKg === undefined
      ? undefined
      : chanceOfAtLeast(evaluation.totals, goalKg);
  const tallest = bins.reduce((a, b) => (b.share > a.share ? b : a), {
    fromKg: 0,
    toKg: 0,
    share: 0,
  });
  const label = [
    `Histogram of simulated totals. Bomb-out ${percent(bombOutShare)}.`,
    bins.length
      ? `Totals from ${kg(bins[0]!.fromKg)} to ${kg(bins[bins.length - 1]!.toKg)}, most often ${kg(tallest.fromKg)} to ${kg(tallest.toKg)}.`
      : "",
    goalChance === undefined
      ? ""
      : `${percent(goalChance)} at or above the goal of ${kg(goalKg!)}.`,
  ]
    .filter(Boolean)
    .join(" ");
  const width = 640;
  const height = 190;
  const base = 168;
  const left = 10;
  const bombOutWidth = 30;
  const binsLeft = left + bombOutWidth + 20;
  const step = (width - binsLeft - 10) / Math.max(bins.length, 1);
  const y = scaleLinear()
    .domain([0, Math.max(bombOutShare, ...bins.map((bin) => bin.share))])
    .range([base, 44]);
  const svg = chartSvg(width, height, label).attr("class", "histogram");
  const barHeight = (share: number) => base - y(share);
  svg
    .append("rect")
    .attr("class", "bar bomb")
    .attr("x", left)
    .attr("y", y(bombOutShare))
    .attr("width", bombOutWidth)
    .attr("height", barHeight(bombOutShare))
    .attr("rx", 3);
  svg
    .append("text")
    .attr("class", "tick-label")
    .attr("x", left + bombOutWidth / 2)
    .attr("y", height - 4)
    .attr("text-anchor", "middle")
    .text("0");
  const labelEvery = Math.ceil(bins.length / 9);
  bins.forEach((bin, index) => {
    const x = binsLeft + index * step;
    const kind = goalKg === undefined || bin.reachesGoal ? "reach" : "short";
    svg
      .append("rect")
      .attr("class", `bar ${kind}`)
      .attr("x", x)
      .attr("y", y(bin.share))
      .attr("width", step * 0.88)
      .attr("height", barHeight(bin.share))
      .attr("rx", 3);
    if (index % labelEvery === 0) {
      svg
        .append("text")
        .attr("class", "tick-label")
        .attr("x", x)
        .attr("y", height - 4)
        .text(bin.fromKg);
    }
  });
  const firstReaching = bins.findIndex((bin) => bin.reachesGoal);
  if (goalChance !== undefined && firstReaching >= 0) {
    const x = binsLeft + firstReaching * step - step * 0.06;
    const nearRight = x > width * 0.6;
    svg
      .append("line")
      .attr("class", "goal-line")
      .attr("x1", x)
      .attr("x2", x)
      .attr("y1", 8)
      .attr("y2", base + 2);
    const note = svg
      .append("text")
      .attr("class", "note-label")
      .attr("text-anchor", nearRight ? "end" : "start");
    [`goal ${kg(goalKg!)}`, `${percent(goalChance)} at or above`].forEach(
      (text, index) => {
        note
          .append("tspan")
          .attr("x", nearRight ? x - 4 : x + 4)
          .attr("y", 16)
          .attr("dy", `${index * 1.2}em`)
          .text(text);
      },
    );
  }
  return svg.node()!;
}

function reachChart(evaluation: Evaluation, goalKg?: number) {
  const points = reachCurve(evaluation.totals);
  const width = 300;
  const height = 170;
  const margin = { top: 12, right: 12, bottom: 22, left: 36 };
  const first = points[0];
  const last = points[points.length - 1];
  const goalChance =
    goalKg === undefined
      ? undefined
      : chanceOfAtLeast(evaluation.totals, goalKg);
  const label = [
    "Chance of reaching each total.",
    first && last
      ? `${percent(first.chance)} reach ${kg(first.totalKg)}, falling to ${percent(last.chance)} at ${kg(last.totalKg)}.`
      : "Every simulated meet bombs out.",
    goalChance === undefined
      ? ""
      : `${percent(goalChance)} reach the goal of ${kg(goalKg!)}.`,
  ]
    .filter(Boolean)
    .join(" ");
  const svg = chartSvg(width, height, label);
  if (!first || !last) return svg.node()!;
  const x = scaleLinear()
    .domain([first.totalKg, last.totalKg])
    .range([margin.left, width - margin.right])
    .nice(4);
  const y = scaleLinear()
    .domain([0, 1])
    .range([height - margin.bottom, margin.top]);
  svg
    .append("g")
    .attr("class", "axis")
    .attr("transform", `translate(0,${height - margin.bottom})`)
    .call(axisBottom(x).ticks(4).tickSize(0).tickPadding(6));
  svg
    .append("g")
    .attr("class", "axis")
    .attr("transform", `translate(${margin.left},0)`)
    .call(
      axisLeft(y)
        .tickValues([0, 0.5, 1])
        .tickFormat((value) => `${Number(value) * 100}%`)
        .tickSize(0)
        .tickPadding(6),
    );
  svg
    .append("path")
    .attr("class", "reach-line")
    .attr(
      "d",
      line<{ totalKg: number; chance: number }>()
        .x((point) => x(point.totalKg))
        .y((point) => y(point.chance))
        .curve(curveStepBefore)([
        ...points,
        { totalKg: last.totalKg, chance: 0 },
      ]),
    );
  if (goalChance !== undefined) {
    const gx = x(goalKg!);
    const gy = y(goalChance);
    const nearRight = gx > width * 0.6;
    svg
      .append("line")
      .attr("class", "goal-line")
      .attr("x1", gx)
      .attr("x2", gx)
      .attr("y1", margin.top)
      .attr("y2", height - margin.bottom);
    svg
      .append("circle")
      .attr("class", "goal-dot")
      .attr("cx", gx)
      .attr("cy", gy)
      .attr("r", 5);
    svg
      .append("text")
      .attr("class", "note-label")
      .attr("x", nearRight ? gx - 8 : gx + 8)
      .attr("y", gy - 8)
      .attr("text-anchor", nearRight ? "end" : "start")
      .text(`${kg(goalKg!)}: ${percent(goalChance)}`);
  }
  return svg.node()!;
}

function riskPanel(evaluation: Evaluation) {
  const { lifts, scaleMax, hint } = bombOutRisk(evaluation.lifts);
  const rows = lifts.map((risk) => {
    const row = element("div", "hbar");
    const track = element("div", "track");
    track.setAttribute("aria-hidden", "true");
    for (const [share, kind] of [
      [risk.tooHeavy, "heavy"],
      [risk.technical, "technical"],
    ] as const) {
      const segment = element("div", kind);
      segment.style.width = `${(share / scaleMax) * 100}%`;
      track.append(segment);
    }
    row.append(
      element("span", "", LIFT_NAMES[risk.lift]),
      track,
      element("span", "value", percent(risk.total)),
    );
    return row;
  });
  return panel("Where the bomb-out risk comes from", false, [
    sub(
      `Chance of missing all three attempts, by lift and cause. Bars run to ${percent(scaleMax)}.`,
    ),
    ...rows,
    legend([
      ["too heavy on the day", "heavy"],
      ["technical miss", "technical"],
    ]),
    sub(hint),
  ]);
}

const ORDINALS = ["1st", "2nd", "3rd"];

function marks(made: boolean[], rest = "") {
  const node = element("span", "marks");
  node.setAttribute("role", "img");
  node.setAttribute(
    "aria-label",
    [
      ...made.map((m) => (m ? "made" : "missed")),
      ...(rest ? ["then"] : []),
    ].join(", "),
  );
  for (const m of made)
    node.append(element("span", m ? "y" : "n", m ? "✓" : "✗"));
  if (rest) node.append(rest);
  return node;
}

function attemptPanel(root: HTMLElement, evaluation: Evaluation) {
  const body = element("div", "attempt-body");
  const tabs = element("div", "tabs");
  tabs.setAttribute("role", "group");
  tabs.setAttribute("aria-label", "Lift");
  const show = (lift: Lift) => {
    shownLift.set(root, lift);
    for (const button of tabs.querySelectorAll("button")) {
      button.setAttribute("aria-pressed", String(button.value === lift));
    }
    const view = attemptView(evaluation.lifts[lift]);
    const rates = element("div", "attempt-rates");
    view.makeChances.forEach((chance, index) => {
      const rate = element("div", "");
      rate.append(
        element("b", "", percent(chance)),
        element("span", "", `${ORDINALS[index]} made`),
      );
      rates.append(rate);
    });
    const table = element("table", "paths");
    table.setAttribute("aria-label", `How ${LIFT_NAMES[lift]} attempts go`);
    const addRow = (first: (Node | string)[], share: number) => {
      const row = table.insertRow();
      row.insertCell().append(...first);
      row.insertCell().textContent = percent(share);
    };
    for (const row of view.rows) {
      addRow(
        [
          marks(row.made),
          ` ${row.bestKg === undefined ? "no lift" : kg(row.bestKg)}`,
        ],
        row.share,
      );
    }
    if (view.missedOpenerShare > 0) {
      addRow(
        [marks([false], "…"), " after a missed opener"],
        view.missedOpenerShare,
      );
    }
    body.replaceChildren(rates, table);
  };
  for (const lift of LIFTS) {
    const button = element("button", "", LIFT_NAMES[lift]);
    button.type = "button";
    button.value = lift;
    button.addEventListener("click", () => show(lift));
    tabs.append(button);
  }
  show(shownLift.get(root) ?? "squat");
  return panel("How each attempt goes", false, [tabs, body]);
}

function dayPanel(evaluation: Evaluation) {
  const day = dayView(evaluation);
  const table = element("table", "paths");
  table.setAttribute("aria-label", "Total by kind of day");
  for (const [label, totalKg] of [
    ["Bad day (1 in 10)", day.badKg],
    ["Normal day", day.normalKg],
    ["Good day (1 in 10)", day.goodKg],
    ["Best possible with this plan", day.bestPossibleKg],
  ] as const) {
    const row = table.insertRow();
    row.insertCell().textContent = label;
    row.insertCell().textContent = kg(totalKg);
  }
  return panel("Bad day, normal day, good day", false, [
    sub("Your total when the day goes worse or better than usual."),
    table,
  ]);
}

const deltaSpan = ({ text, direction }: Delta) =>
  element("span", `delta ${direction}`, text);

function whatIfPanel({ changes, bombOutCap, onApply }: AnalyticsData) {
  const intro = sub(
    "Single changes that move your chances most, each tried on the same simulated meets.",
  );
  if (!changes) {
    return panel("What if you change one thing?", true, [
      intro,
      sub("Trying single changes…"),
    ]);
  }
  const picks = whatIfs(changes.current, changes.changes, bombOutCap);
  if (picks.length === 0) {
    return panel("What if you change one thing?", true, [
      intro,
      sub("No single change improves this plan or lowers its bomb-out chance."),
    ]);
  }
  const list = element("ul", "whatif");
  for (const pick of picks) {
    const item = element("li", "");
    const apply = element("button", "", "Apply");
    apply.type = "button";
    apply.setAttribute("aria-label", `Apply ${pick.text}`);
    apply.addEventListener("click", () => onApply(pick.plans));
    item.append(
      element("span", "change", pick.text),
      ...(pick.goal ? [deltaSpan(pick.goal)] : []),
      deltaSpan(pick.bombOut),
      apply,
    );
    list.append(item);
  }
  return panel("What if you change one thing?", true, [intro, list]);
}
