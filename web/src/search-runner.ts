import type { SearchProgress, SearchRequest } from "../pkg/attempt_sim_wasm.js";
import type { SearchReply } from "./engine.ts";

export interface SearchHandlers {
  progress(progress: SearchProgress): void;
  failed(message: string): void;
}

/** Runs one search in its own worker and returns a function that stops it. */
export function startSearch(
  request: SearchRequest,
  handlers: SearchHandlers,
): () => void {
  const worker = new Worker(new URL("./worker.ts", import.meta.url), {
    type: "module",
  });
  const stop = () => worker.terminate();
  worker.addEventListener("message", (event: MessageEvent<SearchReply>) => {
    const reply = event.data;
    if (reply.kind === "failed") {
      stop();
      handlers.failed(reply.message);
      return;
    }
    if (reply.progress.done) stop();
    handlers.progress(reply.progress);
  });
  worker.addEventListener("error", (event) => {
    event.preventDefault();
    stop();
    handlers.failed(event.message || "the search worker could not start");
  });
  worker.postMessage({ kind: "search", request });
  return stop;
}
