import type { DefineResult, DataType, Constraint } from "../types.js";
import { escapeString } from "./value.js";

type RunFn = (smql: string) => Promise<DefineResult>;

export class DefineMachineBuilder {
  private name: string;
  private dataFields: DataFieldDef[] = [];
  private stateNames: string[] = [];
  private initial?: string;
  private terminals: string[] = [];
  private transitions: TransitionDefEntry[] = [];
  private children: ChildDef[] = [];
  private parentMachine?: string;
  private hooks: HookDef[] = [];
  private roles: RoleDef[] = [];
  private run: RunFn;

  constructor(name: string, run: RunFn) {
    this.name = name;
    this.run = run;
  }

  data(name: string, type: DataType, ...constraints: Constraint[]): this {
    this.dataFields.push({ name, type, constraints });
    return this;
  }

  states(...names: string[]): this {
    this.stateNames.push(...names);
    return this;
  }

  initialState(state: string): this {
    this.initial = state;
    return this;
  }

  terminalStates(...states: string[]): this {
    this.terminals.push(...states);
    return this;
  }

  transition(from: string, to: string): TransitionDefBuilder {
    return new TransitionDefBuilder(this, from, to);
  }

  /** @internal */
  _addTransition(entry: TransitionDefEntry): this {
    this.transitions.push(entry);
    return this;
  }

  child(name: string, machine: string, cardinality?: string): this {
    this.children.push({ name, machine, cardinality });
    return this;
  }

  parent(machine: string): this {
    this.parentMachine = machine;
    return this;
  }

  hook(trigger: string): HookDefBuilder {
    return new HookDefBuilder(this, trigger);
  }

  /** @internal */
  _addHook(entry: HookDef): this {
    this.hooks.push(entry);
    return this;
  }

  role(name: string): RoleDefBuilder {
    return new RoleDefBuilder(this, name);
  }

  /** @internal */
  _addRole(entry: RoleDef): this {
    this.roles.push(entry);
    return this;
  }

  toSmql(): string {
    const blocks: string[] = [];

    if (this.dataFields.length > 0) {
      const fields = this.dataFields.map((f) => {
        let s = `    ${f.name} : ${formatDataType(f.type)}`;
        if (f.constraints.length > 0) {
          s += ` -> ${f.constraints.map(formatConstraint).join(", ")}`;
        }
        return s;
      });
      blocks.push(`  DATA {\n${fields.join("\n")}\n  }`);
    }

    if (this.stateNames.length > 0) {
      blocks.push(`  STATES { ${this.stateNames.join(", ")} }`);
    }

    if (this.initial) {
      blocks.push(`  INITIAL STATE ${this.initial}`);
    }

    if (this.terminals.length > 0) {
      blocks.push(`  TERMINAL STATES { ${this.terminals.join(", ")} }`);
    }

    if (this.transitions.length > 0) {
      const defs = this.transitions.map((t) => {
        const bodyLines: string[] = [];
        for (const g of t.guards) bodyLines.push(`    GUARD: ${g}`);
        for (const a of t.actions) bodyLines.push(`    ACTION: ${a}`);
        for (const m of t.mutates) bodyLines.push(`    MUTATE: ${m}`);
        if (t.timeout) bodyLines.push(`    TIMEOUT: ${t.timeout}`);
        for (const p of t.policies) bodyLines.push(`    APPLY POLICY ${p}`);
        if (t.reactive) bodyLines.push(`    REACTIVE WHEN ${t.reactive}`);
        const body = bodyLines.length > 0 ? `\n${bodyLines.join("\n")}\n  ` : " ";
        return `  ${t.from} -> ${t.to} {${body}}`;
      });
      blocks.push(`  TRANSITIONS {\n${defs.join("\n")}\n  }`);
    }

    if (this.children.length > 0) {
      const defs = this.children.map((c) => {
        const card = c.cardinality ?? `LIST(${c.machine})`;
        return `    ${c.name} : ${card}`;
      });
      blocks.push(`  CHILDREN {\n${defs.join("\n")}\n  }`);
    }

    if (this.parentMachine) {
      blocks.push(`  PARENT: ${this.parentMachine}`);
    }

    if (this.hooks.length > 0) {
      const defs = this.hooks.map((h) => {
        const actions = h.actions.map((a) => `    ${a}`).join("\n");
        return `  ${h.trigger} {\n${actions}\n  }`;
      });
      blocks.push(`  HOOKS {\n${defs.join("\n")}\n  }`);
    }

    if (this.roles.length > 0) {
      const defs = this.roles.map((r) => {
        const perms = r.permissions.map((p) => `    ${p}`).join("\n");
        return `  ${r.name} {\n${perms}\n  }`;
      });
      blocks.push(`  ROLES {\n${defs.join("\n")}\n  }`);
    }

    const body = blocks.length > 0 ? `\n${blocks.join("\n")}\n` : "";
    return `DEFINE MACHINE ${this.name} (${body})`;
  }

  execute(): Promise<DefineResult> {
    return this.run(this.toSmql());
  }
}

// --- Sub-builders ---

interface TransitionDefEntry {
  from: string;
  to: string;
  guards: string[];
  actions: string[];
  mutates: string[];
  timeout?: string;
  policies: string[];
  reactive?: string;
}

export class TransitionDefBuilder {
  private parent: DefineMachineBuilder;
  private entry: TransitionDefEntry;

  constructor(parent: DefineMachineBuilder, from: string, to: string) {
    this.parent = parent;
    this.entry = {
      from,
      to,
      guards: [],
      actions: [],
      mutates: [],
      policies: [],
    };
  }

  guard(expr: string): this {
    this.entry.guards.push(expr);
    return this;
  }

  action(actionStr: string): this {
    this.entry.actions.push(actionStr);
    return this;
  }

  mutate(field: string, expr: string): this {
    this.entry.mutates.push(`${field} = ${expr}`);
    return this;
  }

  timeout(duration: string, targetState: string): this {
    this.entry.timeout = `${duration} -> ${targetState}`;
    return this;
  }

  applyPolicy(name: string): this {
    this.entry.policies.push(name);
    return this;
  }

  reactive(condition: string): this {
    this.entry.reactive = condition;
    return this;
  }

  end(): DefineMachineBuilder {
    this.parent._addTransition(this.entry);
    return this.parent;
  }
}

interface HookDef {
  trigger: string;
  actions: string[];
}

export class HookDefBuilder {
  private parent: DefineMachineBuilder;
  private entry: HookDef;

  constructor(parent: DefineMachineBuilder, trigger: string) {
    this.parent = parent;
    this.entry = { trigger, actions: [] };
  }

  action(actionStr: string): this {
    this.entry.actions.push(actionStr);
    return this;
  }

  end(): DefineMachineBuilder {
    this.parent._addHook(this.entry);
    return this.parent;
  }
}

interface RoleDef {
  name: string;
  permissions: string[];
}

export class RoleDefBuilder {
  private parent: DefineMachineBuilder;
  private entry: RoleDef;

  constructor(parent: DefineMachineBuilder, name: string) {
    this.parent = parent;
    this.entry = { name, permissions: [] };
  }

  canSpawn(): this {
    this.entry.permissions.push("CAN SPAWN");
    return this;
  }

  canTransition(...states: string[]): this {
    if (states.length > 0) {
      this.entry.permissions.push(`CAN TRANSITION [${states.join(", ")}]`);
    } else {
      this.entry.permissions.push("CAN TRANSITION");
    }
    return this;
  }

  canQuery(): this {
    this.entry.permissions.push("CAN QUERY");
    return this;
  }

  canAlter(): this {
    this.entry.permissions.push("CAN ALTER");
    return this;
  }

  canAll(): this {
    this.entry.permissions.push("CAN ALL");
    return this;
  }

  canRead(...fields: string[]): this {
    this.entry.permissions.push(`CAN READ { ${fields.join(", ")} }`);
    return this;
  }

  canWrite(...fields: string[]): this {
    this.entry.permissions.push(`CAN WRITE { ${fields.join(", ")} }`);
    return this;
  }

  cannotRead(...fields: string[]): this {
    this.entry.permissions.push(`CANNOT READ { ${fields.join(", ")} }`);
    return this;
  }

  cannotWrite(...fields: string[]): this {
    this.entry.permissions.push(`CANNOT WRITE { ${fields.join(", ")} }`);
    return this;
  }

  end(): DefineMachineBuilder {
    this.parent._addRole(this.entry);
    return this.parent;
  }
}

// --- Helpers ---

interface DataFieldDef {
  name: string;
  type: DataType;
  constraints: Constraint[];
}

interface ChildDef {
  name: string;
  machine: string;
  cardinality?: string;
}

function formatDataType(t: DataType): string {
  if (typeof t === "string") return t;
  switch (t.type) {
    case "ENUM":
      return `ENUM(${t.variants.join(", ")})`;
    case "REF":
      return `REF(${t.target})`;
    case "LIST":
      return `LIST(${formatDataType(t.inner)})`;
    case "SET":
      return `SET(${formatDataType(t.inner)})`;
    case "MAP":
      return `MAP(${formatDataType(t.key)}, ${formatDataType(t.value)})`;
    case "MONEY":
      return `MONEY(${t.currency})`;
  }
}

function formatConstraint(c: Constraint): string {
  if (typeof c === "string") return c;
  switch (c.type) {
    case "MAX":
      return `MAX(${c.value})`;
    case "MIN":
      return `MIN(${c.value})`;
    case "RANGE":
      return `RANGE(${c.lo}, ${c.hi})`;
    case "DEFAULT":
      if (c.value === null) return "DEFAULT(NULL)";
      if (typeof c.value === "string") return `DEFAULT("${escapeString(c.value)}")`;
      return `DEFAULT(${c.value})`;
    case "PATTERN":
      return `PATTERN("${escapeString(c.regex)}")`;
    case "COMPUTED":
      return `COMPUTED(${c.expr})`;
  }
}
