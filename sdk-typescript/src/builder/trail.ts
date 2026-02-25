import type { TrailResult } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<TrailResult>;

export class TrailBuilder {
  private instanceId: string;
  private actorFilter?: string;
  private fromStateFilter?: string;
  private toStateFilter?: string;
  private sinceExpr?: string;
  private untilExpr?: string;
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

  since(expr: string): this {
    this.sinceExpr = expr;
    return this;
  }

  until(expr: string): this {
    this.untilExpr = expr;
    return this;
  }

  toSmql(): string {
    let s = `TRAIL OF "${escapeString(this.instanceId)}"`;

    const filters: string[] = [];
    if (this.actorFilter) filters.push(`ACTOR ${this.actorFilter}`);
    if (this.fromStateFilter) filters.push(`FROM ${this.fromStateFilter}`);
    if (this.toStateFilter) filters.push(`TO ${this.toStateFilter}`);
    if (this.sinceExpr) filters.push(`SINCE ${this.sinceExpr}`);
    if (this.untilExpr) filters.push(`UNTIL ${this.untilExpr}`);

    if (filters.length > 0) {
      s += ` WHERE ${filters.join(", ")}`;
    }

    return s;
  }

  execute(): Promise<TrailResult> {
    return this.run(this.toSmql());
  }
}
