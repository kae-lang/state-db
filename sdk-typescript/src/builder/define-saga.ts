import type { DefineResult } from "../types.js";

type RunFn = (smql: string) => Promise<DefineResult>;

interface SagaStepDef {
  name: string;
  transition: string;
  when?: string;
  compensate?: string;
}

export class DefineSagaBuilder {
  private name: string;
  private triggerClause?: string;
  private steps: SagaStepDef[] = [];
  private onCompleteActions: string[] = [];
  private onFailureActions: string[] = [];
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  triggerOnEnter(state: string, machine: string): this {
    this.triggerClause = `TRIGGER ON ENTER ${state} ON ${machine}`;
    return this;
  }

  triggerOnSpawn(machine: string): this {
    this.triggerClause = `TRIGGER ON SPAWN ${machine}`;
    return this;
  }

  triggerManual(): this {
    this.triggerClause = "TRIGGER MANUAL";
    return this;
  }

  step(name: string): SagaStepBuilder {
    return new SagaStepBuilder(this, name);
  }

  /** @internal */
  _addStep(step: SagaStepDef): this {
    this.steps.push(step);
    return this;
  }

  onComplete(action: string): this {
    this.onCompleteActions.push(action);
    return this;
  }

  onFailure(action: string): this {
    this.onFailureActions.push(action);
    return this;
  }

  toSmql(): string {
    const lines: string[] = [];

    for (const step of this.steps) {
      let line = `  STEP ${step.name} ${step.transition}`;
      if (step.when) line += ` WHEN ${step.when}`;
      if (step.compensate) line += ` COMPENSATE ${step.compensate}`;
      lines.push(line);
    }

    for (const a of this.onCompleteActions) {
      lines.push(`  ON COMPLETE: ${a}`);
    }

    for (const a of this.onFailureActions) {
      lines.push(`  ON FAILURE: ${a}`);
    }

    return `DEFINE SAGA ${this.name} ${this.triggerClause} {\n${lines.join("\n")}\n}`;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}

export class SagaStepBuilder {
  private parent: DefineSagaBuilder;
  private stepName: string;
  private transitionStr?: string;
  private whenExpr?: string;
  private compensateStr?: string;

  constructor(parent: DefineSagaBuilder, name: string) {
    this.parent = parent;
    this.stepName = name;
  }

  transition(machine: string, instanceExpr: string, toState: string): this {
    this.transitionStr = `TRANSITION ${machine} ${instanceExpr} TO ${toState}`;
    return this;
  }

  when(condition: string): this {
    this.whenExpr = condition;
    return this;
  }

  compensate(machine: string, instanceExpr: string, toState: string): this {
    this.compensateStr = `${machine} ${instanceExpr} TO ${toState}`;
    return this;
  }

  end(): DefineSagaBuilder {
    this.parent._addStep({
      name: this.stepName,
      transition: this.transitionStr!,
      when: this.whenExpr,
      compensate: this.compensateStr,
    });
    return this.parent;
  }
}
