import type { SmqlValue, BatchTransitionResult } from "../types.js";
import { escapeString, valueToSmql } from "./value.js";

type RunFn = (smql: string) => Promise<BatchTransitionResult>;

export class BatchTransitionBuilder {
  private machine: string;
  private filterExpr?: string;
  private toState?: string;
  private withData: [string, SmqlValue][] = [];
  private memoText?: string;
  private actor?: string;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  where(expr: string): this {
    this.filterExpr = expr;
    return this;
  }

  to(state: string): this {
    this.toState = state;
    return this;
  }

  with(data: Record<string, SmqlValue>): this {
    for (const [k, v] of Object.entries(data)) {
      this.withData.push([k, v]);
    }
    return this;
  }

  memo(text: string): this {
    this.memoText = text;
    return this;
  }

  asActor(actor: string): this {
    this.actor = actor;
    return this;
  }

  toSmql(): string {
    let s = `TRANSITION ALL ${this.machine}`;

    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }

    if (this.toState) {
      s += ` TO ${this.toState}`;
    }

    if (this.withData.length > 0) {
      const fields = this.withData.map(
        ([k, v]) => `${k}: ${valueToSmql(v)}`,
      );
      s += ` WITH { ${fields.join(", ")} }`;
    }

    if (this.memoText !== undefined) {
      s += ` MEMO "${escapeString(this.memoText)}"`;
    }

    if (this.actor !== undefined) {
      s += ` AS "${escapeString(this.actor)}"`;
    }

    return s;
  }

  execute(): Promise<BatchTransitionResult> {
    return this.run(this.toSmql());
  }
}
