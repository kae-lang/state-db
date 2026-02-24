import type { SubscriptionEvent, SubscribeOptions } from "./types.js";
import { SubscriptionError } from "./errors.js";

type EventHandler = (event: SubscriptionEvent) => void;

export class SmqlSubscription {
  private url: string;
  private token?: string;
  private ws: WebSocket | null = null;
  private handlers = new Map<string, Set<EventHandler>>();
  private anyHandlers = new Set<EventHandler>();
  private _connected = false;

  constructor(baseUrl: string, options?: SubscribeOptions & { token?: string }) {
    // Convert http(s) to ws(s)
    const wsUrl = baseUrl
      .replace(/^http:/, "ws:")
      .replace(/^https:/, "wss:");

    let url = `${wsUrl}/subscribe`;
    const params: string[] = [];
    if (options?.machine) params.push(`machine=${encodeURIComponent(options.machine)}`);
    if (options?.event) params.push(`event=${encodeURIComponent(options.event)}`);
    if (options?.token) {
      this.token = options.token;
      params.push(`token=${encodeURIComponent(options.token)}`);
    }
    if (params.length > 0) url += `?${params.join("&")}`;

    this.url = url;
  }

  get connected(): boolean {
    return this._connected;
  }

  connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      try {
        this.ws = new WebSocket(this.url);
      } catch (err) {
        reject(new SubscriptionError(`Failed to create WebSocket: ${err}`));
        return;
      }

      this.ws.onopen = () => {
        this._connected = true;
        resolve();
      };

      this.ws.onerror = (event) => {
        if (!this._connected) {
          reject(new SubscriptionError("WebSocket connection failed"));
        }
      };

      this.ws.onclose = () => {
        this._connected = false;
      };

      this.ws.onmessage = (event) => {
        try {
          const data: SubscriptionEvent = JSON.parse(
            typeof event.data === "string" ? event.data : "",
          );
          this.dispatch(data);
        } catch {
          // Ignore malformed messages
        }
      };
    });
  }

  on(event: string, handler: EventHandler): () => void {
    let set = this.handlers.get(event);
    if (!set) {
      set = new Set();
      this.handlers.set(event, set);
    }
    set.add(handler);
    return () => {
      set!.delete(handler);
    };
  }

  onAny(handler: EventHandler): () => void {
    this.anyHandlers.add(handler);
    return () => {
      this.anyHandlers.delete(handler);
    };
  }

  close(): void {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
      this._connected = false;
    }
  }

  private dispatch(event: SubscriptionEvent): void {
    const handlers = this.handlers.get(event.event);
    if (handlers) {
      for (const h of handlers) h(event);
    }
    for (const h of this.anyHandlers) h(event);
  }
}
