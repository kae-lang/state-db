import type { SmqlValue, Instance } from "../types.js";
import { formatDataFields } from "./value.js";

type RunFn = (smql: string) => Promise<Instance>;

export class SpawnBuilder {
  private machine: string;
  private data: Record<string, SmqlValue> = {};
  private thenTransition?: string;
  private run: RunFn;

  constructor(machine: string, run: RunFn) {
    this.machine = machine;
    this.run = run;
  }

  set(data: Record<string, SmqlValue>): this;
  set(key: string, value: SmqlValue): this;
  set(keyOrData: string | Record<string, SmqlValue>, value?: SmqlValue): this {
    if (typeof keyOrData === "string") {
      this.data[keyOrData] = value!;
    } else {
      Object.assign(this.data, keyOrData);
    }
    return this;
  }

  thenTransitionTo(state: string): this {
    this.thenTransition = state;
    return this;
  }

  toSmql(): string {
    let s = `SPAWN ${this.machine} {${formatDataFields(this.data)}}`;
    if (this.thenTransition) {
      s += ` THEN TRANSITION TO ${this.thenTransition}`;
    }
    return s;
  }

  execute(): Promise<Instance> {
    return this.run(this.toSmql());
  }
}
