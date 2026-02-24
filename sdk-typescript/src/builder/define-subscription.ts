import type { DefineResult } from "../types.js";

type RunFn = (smql: string) => Promise<DefineResult>;

export class DefineSubscriptionBuilder {
  private name: string;
  private eventClause?: string;
  private actions: string[] = [];
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  onEnter(state: string, machine: string): this {
    this.eventClause = `ON ENTER ${state} ON ${machine}`;
    return this;
  }

  onExit(state: string, machine: string): this {
    this.eventClause = `ON EXIT ${state} ON ${machine}`;
    return this;
  }

  onSpawn(machine: string): this {
    this.eventClause = `ON SPAWN ${machine}`;
    return this;
  }

  onTransition(machine: string, from?: string, to?: string): this {
    let clause = `ON TRANSITION ${machine}`;
    if (from) clause += ` FROM ${from}`;
    if (to) clause += ` TO ${to}`;
    this.eventClause = clause;
    return this;
  }

  action(actionStr: string): this {
    this.actions.push(`ACTION: ${actionStr}`);
    return this;
  }

  actionWhen(condition: string, actionStr: string): this {
    this.actions.push(`ACTION WHEN ${condition}: ${actionStr}`);
    return this;
  }

  toSmql(): string {
    const lines = this.actions.map((a) => `  ${a}`);
    return `DEFINE SUBSCRIPTION ${this.name} ${this.eventClause} {\n${lines.join("\n")}\n}`;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}
