import type { DefineResult } from "../types.js";

type RunFn = (smql: string) => Promise<DefineResult>;

export class DefinePolicyBuilder {
  private name: string;
  private guards: string[] = [];
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  guard(expr: string): this {
    this.guards.push(expr);
    return this;
  }

  toSmql(): string {
    const lines = this.guards.map((g) => `  GUARD: ${g}`);
    return `DEFINE POLICY ${this.name} {\n${lines.join("\n")}\n}`;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}
