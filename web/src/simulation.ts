import { failed, type Jobs, type Reply } from "./engine.ts";
import { LatestOnly } from "./latest-only.ts";

export interface Simulation<K extends keyof Jobs> {
  started(request: Jobs[K]["request"]): void;
  finished(
    reply: Reply<K>,
    request: Jobs[K]["request"],
    elapsedMs: number,
  ): void;
}

/**
 * Starts a worker for one kind of job and returns a function that submits requests to
 * it. Only the newest request runs next, and replies to replaced requests are dropped.
 */
export function simulationWorker<K extends keyof Jobs>(
  kind: K,
  simulation: Simulation<K>,
): (request: Jobs[K]["request"]) => void {
  const worker = new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
  });
  let startedAt = 0;
  const runs = new LatestOnly<Jobs[K]["request"]>((request) => {
    startedAt = performance.now();
    simulation.started(request);
    worker.postMessage({ kind, request });
  });
  const finish = (reply: Reply<K>) => {
    const elapsedMs = performance.now() - startedAt;
    const request = runs.finish();
    if (request) simulation.finished(reply, request, elapsedMs);
  };

  worker.addEventListener("message", (event: MessageEvent<Reply<K>>) =>
    finish(event.data),
  );
  worker.addEventListener("error", (event) => {
    event.preventDefault();
    finish(failed(event.message || "the simulation worker could not start"));
  });
  worker.addEventListener("messageerror", () =>
    finish(failed("the simulation result could not be read")),
  );
  return (request) => runs.submit(request);
}
