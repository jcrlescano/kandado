// Routes messages to the worker, checking who sent them.
//
// Extension pages (the popup, the settings tabs, and the dropdown and prompt
// iframes, which live on the extension's origin) may ask for anything. A content
// script runs inside a web page, which the person does not control, so it gets a
// short allow-list, and the URL it asks about is the frame's real address, never
// the one it supplied.

import {
  CONTENT_SCRIPT_ALLOWED,
  type ErrorCode,
  REQUEST_TYPES,
  type Reply,
  type Request,
  type Responses,
} from "../shared/messages";
import { ApiError } from "./api";
import type { FrameRef } from "./autofill";
import { type Worker, WorkerError } from "./worker";

export interface Sender {
  id?: string;
  url?: string;
  tab?: { id?: number };
  frameId?: number;
}

export function isExtensionPage(sender: Sender, extensionId: string, extensionOrigin: string) {
  return sender.id === extensionId && sender.url?.startsWith(extensionOrigin) === true;
}

function codeOf(error: unknown): ErrorCode {
  if (error instanceof WorkerError) return error.code;
  if (error instanceof ApiError) return error.sessionEnded ? "signed_out" : "failed";
  return "failed";
}

export async function route(
  worker: Worker,
  message: unknown,
  sender: Sender,
  extension: { id: string; origin: string },
): Promise<Reply<Responses[Request["type"]]>> {
  if (!isRequest(message) || sender.id !== extension.id) return { ok: false, error: "not_allowed" };

  const request: Request = message;
  let frame: FrameRef | undefined;
  if (!isExtensionPage(sender, extension.id, extension.origin)) {
    // A content script: its frame is what the browser says it is.
    if (
      !CONTENT_SCRIPT_ALLOWED.has(request.type) ||
      !sender.url ||
      sender.tab?.id === undefined ||
      sender.frameId === undefined
    ) {
      return { ok: false, error: "not_allowed" };
    }
    frame = { tabId: sender.tab.id, frameId: sender.frameId, url: sender.url };
  }
  try {
    return { ok: true, value: await worker.handle(request, frame) };
  } catch (error) {
    return { ok: false, error: codeOf(error) };
  }
}

function isRequest(message: unknown): message is Request {
  return (
    typeof message === "object" &&
    message !== null &&
    typeof (message as { type?: unknown }).type === "string" &&
    REQUEST_TYPES.has((message as { type: string }).type)
  );
}
