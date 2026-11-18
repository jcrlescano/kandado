// The link to the Kandado desktop app, over native messaging
// (`kandado-native-host`, then a named pipe; see kandado-bridge). Used for
// "Unlock with desktop app" (E-02), pairing (E-31, D-45), and locking and
// unlocking together. If the host is missing, everything here reports
// "unavailable" and nothing else in the extension changes.

export interface NativePort {
  postMessage(message: unknown): void;
  onMessage(listener: (message: unknown) => void): void;
  onDisconnect(listener: () => void): void;
  disconnect(): void;
}

export type ConnectNative = () => NativePort;

export type DesktopState = "signed_out" | "locked" | "unlocked";
export type Refusal = "locked" | "not_paired" | "turned_off" | "declined" | "invalid";

export interface DesktopStatus {
  /** The desktop app answered. */
  available: boolean;
  state?: DesktopState;
  userId?: string;
  unlockEnabled?: boolean;
}

type Answer = { type: string; [key: string]: unknown };

const QUICK_MS = 5_000;
/** The person has two minutes to confirm a pairing in the app. */
export const PAIRING_MS = 130_000;

export class DesktopLink {
  private port: NativePort | null = null;
  private nextId = 1;
  private readonly waiting = new Map<
    number,
    { resolve: (answer: Answer | null) => void; timer: ReturnType<typeof setTimeout> }
  >();

  constructor(
    private readonly connectNative: ConnectNative | undefined,
    private readonly onEvent: (event: "locked" | "unlocked") => void,
  ) {}

  /** Whether a connection is open (it keeps the worker alive to hear events). */
  get connected(): boolean {
    return this.port !== null;
  }

  private open(): NativePort | null {
    if (this.port) return this.port;
    if (!this.connectNative) return null;
    let port: NativePort;
    try {
      port = this.connectNative();
    } catch {
      return null;
    }
    this.port = port;
    port.onMessage((message) => this.received(message));
    port.onDisconnect(() => {
      if (this.port === port) this.port = null;
      for (const [id, entry] of this.waiting) {
        clearTimeout(entry.timer);
        entry.resolve(null);
        this.waiting.delete(id);
      }
    });
    return port;
  }

  private received(message: unknown): void {
    if (typeof message !== "object" || message === null) return;
    const answer = message as Answer;
    if (answer.type === "unavailable") {
      this.close();
      return;
    }
    if (answer.type === "event") {
      if (answer.event === "locked" || answer.event === "unlocked") this.onEvent(answer.event);
      return;
    }
    const id = typeof answer.id === "number" ? answer.id : -1;
    const entry = this.waiting.get(id);
    if (!entry) return;
    this.waiting.delete(id);
    clearTimeout(entry.timer);
    entry.resolve(answer);
  }

  /** Sends a request; null when the app is not there or did not answer in time. */
  private request(body: Record<string, unknown>, timeoutMs = QUICK_MS): Promise<Answer | null> {
    const port = this.open();
    if (!port) return Promise.resolve(null);
    const id = this.nextId++;
    return new Promise((resolve) => {
      const timer = setTimeout(() => {
        this.waiting.delete(id);
        resolve(null);
      }, timeoutMs);
      this.waiting.set(id, { resolve, timer });
      try {
        port.postMessage({ id, ...body });
      } catch {
        clearTimeout(timer);
        this.waiting.delete(id);
        resolve(null);
      }
    });
  }

  async status(): Promise<DesktopStatus> {
    const answer = await this.request({ type: "status" });
    if (answer?.type !== "status") return { available: false };
    return {
      available: true,
      state: answer.state as DesktopState,
      userId: typeof answer.user_id === "string" ? answer.user_id : undefined,
      unlockEnabled: answer.unlock_enabled === true,
    };
  }

  /** Waits for the person to accept in the app. */
  async pair(
    pairingId: string,
    publicKey: string,
    browser: string,
  ): Promise<"paired" | Refusal | "unavailable"> {
    const answer = await this.request(
      { type: "pair", pairing_id: pairingId, public_key: publicKey, browser },
      PAIRING_MS,
    );
    if (!answer) return "unavailable";
    if (answer.type === "paired") return "paired";
    return (answer.reason as Refusal) ?? "invalid";
  }

  /** The user key sealed to this pairing, base64. */
  async unlock(pairingId: string): Promise<{ sealed: string } | Refusal | "unavailable"> {
    const answer = await this.request({ type: "unlock", pairing_id: pairingId });
    if (!answer) return "unavailable";
    if (answer.type === "unlocked" && typeof answer.sealed === "string") {
      return { sealed: answer.sealed };
    }
    return (answer.reason as Refusal) ?? "invalid";
  }

  async unpair(pairingId: string): Promise<void> {
    await this.request({ type: "unpair", pairing_id: pairingId });
  }

  close(): void {
    const port = this.port;
    this.port = null;
    port?.disconnect();
  }
}
