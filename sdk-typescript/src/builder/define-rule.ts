import type { DefineResult } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<DefineResult>;

export class DefineRuleBuilder {
  private name: string;
  private triggerClause?: string;
  private invariantExpr?: string;
  private messageText?: string;
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  beforeTransition(machine: string): this {
    this.triggerClause = `BEFORE TRANSITION ON ${machine}`;
    return this;
  }

  beforeSpawn(machine: string): this {
    this.triggerClause = `BEFORE SPAWN ON ${machine}`;
    return this;
  }

  beforeAnyTransition(): this {
    this.triggerClause = "BEFORE ANY TRANSITION";
    return this;
  }

  afterTransition(machine: string): this {
    this.triggerClause = `AFTER TRANSITION ON ${machine}`;
    return this;
  }

  invariant(expr: string, message?: string): this {
    this.invariantExpr = expr;
    if (message) this.messageText = message;
    return this;
  }

  toSmql(): string {
    const lines: string[] = [];
    if (this.invariantExpr) {
      lines.push(`  INVARIANT: ${this.invariantExpr}`);
    }
    if (this.messageText) {
      lines.push(`  MESSAGE: "${escapeString(this.messageText)}"`);
    }
    return `DEFINE RULE ${this.name} ${this.triggerClause} {\n${lines.join("\n")}\n}`;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}
