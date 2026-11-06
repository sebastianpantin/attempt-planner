import type {
  ReplanRequest,
  ReplanResponse,
  SearchRequest,
} from "../pkg/attempt_sim_wasm.js";
import { element } from "./dom.ts";
import * as format from "./format.ts";
import { LIFT_NAMES } from "./format.ts";
import { LIFTS } from "./inputs.ts";
import {
  afterNext,
  ATTEMPTS_PER_LIFT,
  basis,
  bombedOut,
  byLift,
  describeSetup,
  describeTaken,
  lightestNextKg,
  loadMeet,
  loadProblem,
  type Meet,
  replanRequest,
  saveMeet,
  slotName,
  slotOf,
  startMeet,
  take,
  undo,
} from "./meet-day.ts";
import { describeProblem } from "./problems.ts";
import { simulationWorker } from "./simulation.ts";

const byId = (id: string) => document.getElementById(id)!;
const button = (id: string) => byId(id) as HTMLButtonElement;

/** The plan as the planner shows it, and what it still needs before a meet can start. */
export interface CurrentPlan {
  request: SearchRequest;
  /** Empty when the plan can start a meet. */
  missing: string;
}

/**
 * Sets up the meet day view: the attempts taken so far, kept in the browser, and the rest
 * of the meet replanned after each one. A new meet starts from `currentPlan()`.
 */
export function setUpMeetDay(currentPlan: () => CurrentPlan) {
  const planner = byId("planner");
  const view = byId("meet-day");
  const loadInput = byId("meet-load") as HTMLInputElement;
  const loadProblemText = byId("meet-load-problem");
  const progress = byId("meet-progress");
  let meet = loadMeet(localStorage);
  /** The replan request whose answer is on screen, so reopening the view does not redo it. */
  let shownFor: string | undefined;
  /** Whether the lifter changed the load since the replan last filled it in. */
  let loadEdited = false;

  const status = (text: string) => {
    byId("meet-status").textContent = text;
  };

  function show(meetDay: boolean) {
    planner.hidden = meetDay;
    view.hidden = !meetDay;
    document.body.dataset["view"] = meetDay ? "meet" : "plan";
    button("to-meet-day").hidden = meetDay;
    button("to-planner").hidden = !meetDay;
    if (meetDay) render();
  }

  function setLoadProblem(problem: string | undefined) {
    loadProblemText.textContent = problem ?? "";
    loadInput.toggleAttribute("aria-invalid", problem !== undefined);
  }

  function render() {
    byId("meet-start").hidden = meet !== undefined;
    progress.hidden = meet === undefined;
    if (!meet) {
      const { request, missing } = currentPlan();
      byId("meet-start-plan").textContent =
        missing || describeSetup(startMeet(request).setup);
      button("start-meet").disabled = missing !== "";
      return;
    }
    const { taken } = meet;
    byId("meet-plan").textContent = describeSetup(meet.setup);
    const slot = slotOf(taken.length);
    byId("meet-entry").hidden = !slot;
    if (slot) {
      byId("meet-entry-legend").textContent =
        `How did ${slotName(slot).toLowerCase()} go?`;
      loadInput.min = String(lightestNextKg(taken));
      loadInput.setAttribute("aria-label", `${slotName(slot)} load (kg)`);
    }
    const last = taken.at(-1);
    const undoButton = button("undo-attempt");
    undoButton.hidden = !last;
    undoButton.textContent = last
      ? `Undo ${describeTaken(taken.length - 1, last).toLowerCase()}`
      : "";
    const bombs = bombedOut(taken);
    byId("meet-warning").textContent =
      bombs.length > 0
        ? `Three misses on the ${format.listOf(bombs.map((lift) => LIFT_NAMES[lift].full.toLowerCase()))}: no total at this meet.`
        : "";
    renderLog();
    const request = replanRequest(meet);
    if (JSON.stringify(request) === shownFor) return;
    view.setAttribute("aria-busy", "true");
    submitReplan(request);
  }

  function renderLog() {
    const lifts = byLift(meet?.taken ?? []);
    byId("meet-log").replaceChildren(
      ...LIFTS.map((lift) => {
        const row = element("tr");
        const heading = element("th", "", LIFT_NAMES[lift].full);
        heading.scope = "row";
        row.append(heading);
        for (let index = 0; index < ATTEMPTS_PER_LIFT; index++) {
          const cell = row.insertCell();
          const attempt = lifts[lift][index];
          if (!attempt) continue;
          cell.className = attempt.made ? "tag make" : "tag miss";
          cell.append(
            element("span", "", attempt.made ? "✓ " : "✗ "),
            format.weight(attempt.weightKg),
            element(
              "span",
              "visually-hidden",
              attempt.made ? " kg, made" : " kg, missed",
            ),
          );
          cell.firstElementChild!.setAttribute("aria-hidden", "true");
        }
        return row;
      }),
    );
  }

  function showReplan(reply: ReplanResponse) {
    byId("meet-problems").replaceChildren(
      ...(reply.kind === "invalid" ? reply.problems : []).map((problem) =>
        element("li", "", describeProblem(problem)),
      ),
    );
    if (reply.kind !== "replanned" || !meet) return;
    const { result, next } = reply;
    const goalChance = result.goalProbability;
    byId("meet-goal-stat").hidden = goalChance === undefined;
    byId("meet-goal-chance").textContent =
      goalChance === undefined ? "" : format.percent(goalChance);
    byId("meet-bomb-out").textContent = format.percent(
      result.bombOutProbability,
    );
    byId("meet-average").textContent = format.kg(result.expectedTotalKg);
    byId("meet-basis").textContent = basis(reply, meet.taken.length);
    byId("meet-next-heading").textContent = next ? "Next attempt" : "Meet over";
    byId("meet-next-slot").textContent = next ? slotName(next) : "Your total";
    byId("meet-next-kg").textContent = format.kg(
      next ? next.weightKg : result.expectedTotalKg,
    );
    byId("meet-after-next").textContent = next
      ? afterNext(result.plans, next)
      : "";
    if (next && !loadEdited) {
      loadInput.valueAsNumber = next.weightKg;
      setLoadProblem(undefined);
    }
    status(
      next
        ? `Next: ${slotName(next).toLowerCase()}, ${format.kg(next.weightKg)}.`
        : `Meet over: ${format.kg(result.expectedTotalKg)} total.`,
    );
  }

  const submitReplan = simulationWorker("replan", {
    started: () => {},
    finished(reply, request: ReplanRequest) {
      if (!meet) return;
      view.removeAttribute("aria-busy");
      if ("message" in reply) {
        const failure = `The replan failed: ${reply.message}`;
        byId("meet-problems").replaceChildren(element("li", "", failure));
        status(failure);
        return;
      }
      shownFor = JSON.stringify(request);
      showReplan(reply);
    },
  });

  function update(next: Meet | undefined) {
    meet = next;
    saveMeet(localStorage, meet);
    loadEdited = false;
    shownFor = undefined;
    render();
  }

  function record(made: boolean) {
    if (!meet) return;
    const weightKg = loadInput.valueAsNumber;
    const problem = loadProblem(weightKg, meet.taken);
    setLoadProblem(problem);
    if (problem) {
      loadInput.focus();
      return;
    }
    const index = meet.taken.length;
    const attempt = { weightKg, made };
    update(take(meet, attempt));
    status(`Saved ${describeTaken(index, attempt).toLowerCase()}. Replanning…`);
  }

  button("to-meet-day").addEventListener("click", () => {
    show(true);
    byId("meet-day-heading").focus();
  });
  button("to-planner").addEventListener("click", () => show(false));
  button("start-meet").addEventListener("click", () => {
    update(startMeet(currentPlan().request));
    loadInput.focus();
  });
  view.querySelector(".meet-results")!.addEventListener("click", (event) => {
    const pressed = (event.target as Element).closest("button");
    if (pressed) record(pressed.value === "made");
  });
  byId("meet-entry").addEventListener("submit", (event) =>
    event.preventDefault(),
  );
  loadInput.addEventListener("input", () => {
    loadEdited = true;
    setLoadProblem(undefined);
  });
  button("undo-attempt").addEventListener("click", () => {
    if (!meet) return;
    const last = meet.taken.at(-1);
    update(undo(meet));
    if (last) {
      loadInput.valueAsNumber = last.weightKg;
      loadEdited = true;
      status(`Undid ${describeTaken(meet.taken.length, last).toLowerCase()}.`);
    }
    loadInput.focus();
  });
  button("end-meet").addEventListener("click", () => {
    if (!confirm("Clear every attempt of this meet?")) return;
    update(undefined);
    status("");
    button("start-meet").focus();
  });

  show(meet !== undefined);
}
