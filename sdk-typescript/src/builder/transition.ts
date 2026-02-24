import type { SmqlValue, TransitionResult, TryTransitionResult } from "../types.js";
import { escapeString, valueToSmql } from "./value.js";

type RunFn<T> = (smql: string) => Promise<T>;

export class TransitionBuilder<T = TransitionResult> {
  private machine: string;
  private instanceId: string;
  private toState: string;
  private isTry: boolean;
  private withData: [string, SmqlValue][] = [];
  private memoText?: string;
  private actor?: string;
  private throughStates: string[] = [];
  private orStayFlag = false;
  private cascadeFlag = false;
  private run: RunFn<T>;

  constructor(
    machine: string,
    instanceId: string,
    toState: string,
    isTry: boolean,
    run: RunFn<T>,
  ) {
    this.machine = machine;
    this.instanceId = instanceId;
    this.toState = toState;
    this.isTry = isTry;
    this.run = run;
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

  through(states: string[]): this {
    this.throughStates = states;
    return this;
  }

  orStay(): this {
    this.orStayFlag = true;
    return this;
  }

  cascade(): this {
    this.cascadeFlag = true;
    return this;
  }

  toSmql(): string {
    let s = `TRANSITION ${this.machine} "${escapeString(this.instanceId)}" TO ${this.toState}`;

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

    if (this.throughStates.length > 0) {
      s += ` THROUGH [${this.throughStates.join(", ")}]`;
    }

    if (this.orStayFlag) {
      s += " OR_STAY";
    }

    if (this.cascadeFlag) {
      s += " CASCADE";
    }

    if (this.isTry) {
      s = `TRY ${s}`;
    }

    return s;
  }

  execute(): Promise<T> {
    return this.run(this.toSmql());
  }
}
