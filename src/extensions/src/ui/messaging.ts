// Asking the service worker. Pages never touch keys or rows themselves.
import type { ErrorCode, Reply, Request, Responses } from "../shared/messages";

export class AskError extends Error {
  constructor(readonly code: ErrorCode) {
    super(code);
    this.name = "AskError";
  }
}

export async function ask<R extends Request>(request: R): Promise<Responses[R["type"]]> {
  const reply = (await chrome.runtime.sendMessage(request)) as
    | Reply<Responses[R["type"]]>
    | undefined;
  if (!reply) throw new AskError("failed");
  if (!reply.ok) throw new AskError(reply.error);
  return reply.value;
}
