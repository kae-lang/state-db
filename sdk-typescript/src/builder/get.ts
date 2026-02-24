import type { Instance } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<Instance>;

export class GetBuilder {
  private machine: string;
  private instanceId: string;
  private actorRole?: string;
  private run: RunFn;

  constructor(machine: string, instanceId: string, run: RunFn) {
    this.machine = machine;
    this.instanceId = instanceId;
    this.run = run;
  }

  asActor(role: string): this {
    this.actorRole = role;
    return this;
  }

  toSmql(): string {
    let s = `GET ${this.machine} "${escapeString(this.instanceId)}"`;
    if (this.actorRole) {
      s += ` AS ACTOR ${this.actorRole}`;
    }
    return s;
  }

  execute(): Promise<Instance> {
    return this.run(this.toSmql());
  }
}
