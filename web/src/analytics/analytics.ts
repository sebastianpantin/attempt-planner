import type {
  ChangesResponse,
  Evaluation,
  Lift,
  LiftPlanKg,
  PerLift,
} from "../../pkg/attempt_sim_wasm.js";
import { axisBottom, axisLeft } from "d3-axis";
import { scaleLinear } from "d3-scale";
import { select, type Selection } from "d3-selection";
import { curveStepBefore, line } from "d3-shape";
import { element } from "../dom.ts";
import { kg, LIFT_NAMES, percent } from "../format.ts";
import { LIFTS } from "../inputs.ts";
import { type KnownGoal, knownGoal } from "../outcome.ts";
import {
  appliedLine,
  attemptView,
  bombOutRisk,
  chanceOfAtLeast,
  dayView,
  histogram,
  histogramLabel,
  labelEvery,
  MAKE_CHANCE_LABELS,
  pathText,
  reachCurve,
  reachLabel,
  totalCheck,
  totalCheckRange,
  whatIfs,
  type Delta,
} from "./analytics-data.ts";
import "./analytics.css";

export interface AnalyticsData {
  evaluation: Evaluation;
  goalKg?: number;
  changes?: Extract<ChangesResponse, { kind: "changed" }>;
  onApply: (plans: PerLift<LiftPlanKg>) => void;
}

const SVG_NS = "http://www.w3.org/2000/svg";

/** The lift shown in "How each attempt goes", kept across renders. */
let shownLift: Lift = "squat";
/** The total last checked on the reach curve, kept across renders. */
let checkedTotalKg: number | undefined;
/** The what-if last applied, confirmed in its panel while that line keeps focus. */
let applied: string | undefined;

/**
 * Fills `root` with the plan analytics panels, replacing what was there. A focused
 * confirmation of an applied what-if keeps focus through the re-render.
 */
export function renderAnalytics(root: HTMLElement, data: AnalyticsData) {
  const confirmationFocused =
    document.activeElement?.classList.contains("applied") &&
    root.contains(document.activeElement);
  if (!confirmationFocused) applied = undefined;
  const { evaluation } = data;
  const goal = knownGoal(evaluation, data.goalKg);
  const container = element("div", "analytics");
  container.append(
    panel("Where your total lands", true, [
      sub(
        "Share of simulated meets by total. Bomb-outs count as 0 and are shown on the left.",
      ),
      histogramChart(evaluation, goal),
      legend([
        ...(goal
          ? [
              ["reaches the goal", "reach"],
              ["short of the goal", "short"],
            ]
          : [["total", "reach"]]),
        ["bomb-out", "bomb"],
      ]),
    ]),
    reachPanel(evaluation, goal),
    riskPanel(evaluation),
    attemptPanel(evaluation, data.changes?.current.plans),
    dayPanel(evaluation),
    whatIfPanel(data),
  );
  root.replaceChildren(container);
  if (confirmationFocused) root.querySelector<HTMLElement>(".applied")?.focus();
}

function panel(title: string, wide: boolean, children: Node[]) {
  const section = element("section", wide ? "panel wide" : "panel");
  section.append(element("h3", "", title), ...children);
  return section;
}

const sub = (text: string) => element("p", "sub", text);

function legend(items: string[][]) {
  const node = element("div", "legend");
  for (const [label, swatch] of items) {
    const item = element("span");
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

type Svg = Selection<SVGSVGElement, unknown, null, undefined>;

/** A dashed line at the goal with its label, turned left near the right edge of the chart. */
function goalMarker(
  svg: Svg,
  at: { x: number; top: number; bottom: number; labelY: number },
  chartWidth: number,
  lines: string[],
) {
  const nearRight = at.x > chartWidth * 0.6;
  svg
    .append("line")
    .attr("class", "goal-line")
    .attr("x1", at.x)
    .attr("x2", at.x)
    .attr("y1", at.top)
    .attr("y2", at.bottom);
  const note = svg
    .append("text")
    .attr("class", "note-label")
    .attr("text-anchor", nearRight ? "end" : "start");
  lines.forEach((text, index) => {
    note
      .append("tspan")
      .attr("x", nearRight ? at.x - 6 : at.x + 6)
      .attr("y", at.labelY)
      .attr("dy", `${index * 1.2}em`)
      .text(text);
  });
}

function histogramChart(evaluation: Evaluation, goal?: KnownGoal) {
  const totals = histogram(evaluation.totals, goal?.kg);
  const { bombOutShare, bins } = totals;
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
  const svg = chartSvg(width, height, histogramLabel(totals, goal)).attr(
    "class",
    "histogram",
  );
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
  const every = labelEvery(bins.length);
  bins.forEach((bin, index) => {
    const x = binsLeft + index * step;
    const kind = !goal || bin.reachesGoal ? "reach" : "short";
    svg
      .append("rect")
      .attr("class", `bar ${kind}`)
      .attr("x", x)
      .attr("y", y(bin.share))
      .attr("width", step * 0.88)
      .attr("height", barHeight(bin.share))
      .attr("rx", 3);
    if (index % every === 0) {
      svg
        .append("text")
        .attr("class", "tick-label")
        .attr("x", x)
        .attr("y", height - 4)
        .text(bin.fromKg);
    }
  });
  const firstReaching = bins.findIndex((bin) => bin.reachesGoal);
  if (goal && firstReaching >= 0) {
    goalMarker(
      svg,
      {
        x: binsLeft + firstReaching * step - step * 0.06,
        top: 8,
        bottom: base + 2,
        labelY: 16,
      },
      width,
      [`goal ${kg(goal.kg)}`, `${percent(goal.chance)} at or above`],
    );
  }
  return svg.node()!;
}

function reachPanel(evaluation: Evaluation, goal?: KnownGoal) {
  const { totals } = evaluation;
  const points = reachCurve(totals);
  const title = "Chance of reaching any total";
  const width = 300;
  const height = 170;
  const margin = { top: 12, right: 12, bottom: 22, left: 36 };
  const svg = chartSvg(width, height, reachLabel(points, goal)).attr(
    "class",
    "reach",
  );
  const first = points[0];
  const last = points.at(-1);
  if (!first || !last) {
    return panel(title, false, [
      sub("Chance of a total at or above each weight."),
      svg.node()!,
    ]);
  }
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
  if (goal) {
    goalMarker(
      svg,
      {
        x: x(goal.kg),
        top: margin.top,
        bottom: height - margin.bottom,
        labelY: margin.top + 10,
      },
      width,
      [`goal: ${percent(goal.chance)}`],
    );
  }
  const marker = svg.append("g").attr("class", "check-marker");
  const markerLine = marker
    .append("line")
    .attr("y1", margin.top)
    .attr("y2", height - margin.bottom);
  const markerDot = marker.append("circle").attr("r", 5);

  const slider = element("input");
  slider.type = "range";
  slider.step = "2.5";
  const range = totalCheckRange(
    points,
    checkedTotalKg ?? goal?.kg ?? evaluation.totalPercentilesKg.p50,
  );
  slider.min = String(range.min);
  slider.max = String(range.max);
  const readout = element("span", "check-readout");
  readout.setAttribute("aria-hidden", "true");
  const show = (totalKg: number) => {
    slider.valueAsNumber = totalKg;
    const text = totalCheck(totals, totalKg);
    readout.textContent = text;
    slider.setAttribute("aria-valuetext", text);
    markerLine.attr("x1", x(totalKg)).attr("x2", x(totalKg));
    markerDot
      .attr("cx", x(totalKg))
      .attr("cy", y(chanceOfAtLeast(totals, totalKg)));
  };
  const check = (totalKg: number) => {
    checkedTotalKg = totalCheckRange(points, totalKg).value;
    show(checkedTotalKg);
  };
  show(range.value);
  slider.addEventListener("input", () => check(slider.valueAsNumber));
  const chart = svg.node()!;
  const dragTo = (event: PointerEvent) => {
    const box = chart.getBoundingClientRect();
    check(x.invert(((event.clientX - box.left) / box.width) * width));
  };
  chart.addEventListener("pointerdown", (event) => {
    chart.setPointerCapture(event.pointerId);
    dragTo(event);
  });
  chart.addEventListener("pointermove", (event) => {
    if (chart.hasPointerCapture(event.pointerId)) dragTo(event);
  });
  const label = element("label", "total-check");
  label.append(element("span", "", "Check a total"), slider, readout);
  return panel(title, false, [
    sub(
      "Chance of a total at or above each weight. Drag along the curve to check any total.",
    ),
    chart,
    label,
    sub("The higher the total, the fewer meets reach it."),
  ]);
}

function riskPanel(evaluation: Evaluation) {
  const { lifts, scaleMax, overlap, hint } = bombOutRisk(
    evaluation.lifts,
    evaluation.bombOutProbability,
  );
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
      element("span", "", LIFT_NAMES[risk.lift].short),
      track,
      element("span", "value", percent(risk.total)),
    );
    return row;
  });
  return panel("Where the bomb-out risk comes from", false, [
    sub(
      "A bomb-out is missing all three attempts on one lift: the meet ends with no total.",
    ),
    sub(
      `Each bar is one lift's own chance of a bomb-out, by cause. Bars run to ${percent(scaleMax)}.`,
    ),
    ...rows,
    legend([
      ["too heavy on the day", "heavy"],
      ["technical miss", "technical"],
    ]),
    ...(overlap ? [sub(overlap)] : []),
    sub(hint),
  ]);
}

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

function attemptPanel(evaluation: Evaluation, plans?: PerLift<LiftPlanKg>) {
  const body = element("div", "attempt-body");
  const tabs = element("div", "tabs");
  tabs.setAttribute("role", "group");
  tabs.setAttribute("aria-label", "Lift");
  const show = (lift: Lift) => {
    shownLift = lift;
    for (const button of tabs.querySelectorAll("button")) {
      button.setAttribute("aria-pressed", String(button.value === lift));
    }
    const view = attemptView(evaluation.lifts[lift]);
    const rates = element("div", "attempt-rates");
    view.makeChances.forEach((chance, index) => {
      const rate = element("div");
      rate.append(
        element("b", "", percent(chance)),
        element("span", "", MAKE_CHANCE_LABELS[index]),
      );
      rates.append(rate);
    });
    const table = element("table", "paths");
    table.setAttribute(
      "aria-label",
      `How ${LIFT_NAMES[lift].full} attempts go`,
    );
    const addRow = (first: (Node | string)[], share: number) => {
      const row = table.insertRow();
      row.insertCell().append(...first);
      row.insertCell().textContent = percent(share);
    };
    for (const row of view.rows) {
      addRow([marks(row.made), ` ${pathText(row, plans?.[lift])}`], row.share);
    }
    if (view.missedOpenerShare > 0) {
      addRow(
        [marks([false], "…"), " missed opener, any 2nd and 3rd"],
        view.missedOpenerShare,
      );
    }
    body.replaceChildren(rates, table);
  };
  for (const lift of LIFTS) {
    const button = element("button", "", LIFT_NAMES[lift].short);
    button.type = "button";
    button.value = lift;
    button.addEventListener("click", () => show(lift));
    tabs.append(button);
  }
  show(shownLift);
  return panel("How each attempt goes", false, [
    sub(
      "How often each attempt is made, and every way the three attempts can go with how often it happens.",
    ),
    tabs,
    body,
    sub(
      "✓ made, ✗ missed, in attempt order. The 2nd and 3rd weights depend on whether the attempt before was made, so their make chances count every weight they can be.",
    ),
  ]);
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

function whatIfPanel({ changes, onApply }: AnalyticsData) {
  const intro = sub(
    "Single changes that move your chances most, each tried on the same simulated meets. A 2nd or 3rd attempt is a jump: how much heavier than the attempt before.",
  );
  const confirmation = element(
    "p",
    "applied",
    applied && appliedLine(applied, changes !== undefined),
  );
  confirmation.tabIndex = -1;
  confirmation.setAttribute("role", "status");
  confirmation.hidden = !applied;
  const title = "What if you change one thing?";
  if (!changes) {
    return panel(title, true, [
      intro,
      confirmation,
      sub("Trying single changes…"),
    ]);
  }
  const picks = whatIfs(changes.current, changes.changes);
  if (picks.length === 0) {
    return panel(title, true, [
      intro,
      confirmation,
      sub("No single change improves this plan or lowers its bomb-out chance."),
    ]);
  }
  const list = element("ul", "whatif");
  for (const pick of picks) {
    const item = element("li");
    const apply = element("button", "", "Apply");
    apply.type = "button";
    apply.setAttribute("aria-label", `Apply ${pick.text}`);
    apply.addEventListener("click", () => {
      applied = pick.text;
      confirmation.textContent = appliedLine(applied, false);
      confirmation.hidden = false;
      confirmation.focus();
      onApply(pick.plans);
    });
    item.append(
      element("span", "change", pick.text),
      ...(pick.goal ? [deltaSpan(pick.goal)] : []),
      deltaSpan(pick.bombOut),
      apply,
    );
    list.append(item);
  }
  return panel(title, true, [intro, confirmation, list]);
}
