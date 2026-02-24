import type { TrailResult } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<TrailResult>;

export class TrailBuilder {
  private instanceId: string;
  private actorFilter?: string;
  private fromStateFilter?: string;
  private toStateFilter?: string;
  private run: RunFn;

  constructor(instanceId: string, run: RunFn) {
    this.instanceId = instanceId;
    this.run = run;
  }

  byActor(actor: string): this {
    this.actorFilter = actor;
    return this;
  }

  fromState(state: string): this {
    this.fromStateFilter = state;
    return this;
  }

  toState(state: string): this {
    this.toStateFilter = state;
    return this;
  }

  toSmql(): string {
    let s = `TRAIL OF "${escapeString(this.instanceId)}"`;

    const filters: string[] = [];
    if (this.actorFilter) filters.push(`ACTOR ${this.actorFilter}`);
    if (this.fromStateFilter) filters.push(`FROM ${this.fromStateFilter}`);
    if (this.toStateFilter) filters.push(`TO ${this.toStateFilter}`);

    if (filters.length > 0) {
      s += ` WHERE ${filters.join(", ")}`;
    }

    return s;
  }

  execute(): Promise<TrailResult> {
    return this.run(this.toSmql());
  }
}
