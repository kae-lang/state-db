"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/index.ts
var index_exports = {};
__export(index_exports, {
  AggregateBuilder: () => AggregateBuilder,
  AlterMachineBuilder: () => AlterMachineBuilder,
  BadRequestError: () => BadRequestError,
  BatchTransitionBuilder: () => BatchTransitionBuilder,
  ComparePathsBuilder: () => ComparePathsBuilder,
  ConflictError: () => ConflictError,
  DefineMachineBuilder: () => DefineMachineBuilder,
  DefinePolicyBuilder: () => DefinePolicyBuilder,
  DefineProjectionBuilder: () => DefineProjectionBuilder,
  DefineRuleBuilder: () => DefineRuleBuilder,
  DefineSagaBuilder: () => DefineSagaBuilder,
  DefineSubscriptionBuilder: () => DefineSubscriptionBuilder,
  DefineViewBuilder: () => DefineViewBuilder,
  ExplainTransitionsBuilder: () => ExplainTransitionsBuilder,
  Expr: () => Expr,
  FindBuilder: () => FindBuilder,
  FunnelBuilder: () => FunnelBuilder,
  GetBuilder: () => GetBuilder,
  GetEventsBuilder: () => GetEventsBuilder,
  HookDefBuilder: () => HookDefBuilder,
  NetworkError: () => NetworkError,
  NotFoundError: () => NotFoundError,
  PathsBuilder: () => PathsBuilder,
  RoleDefBuilder: () => RoleDefBuilder,
  SagaStepBuilder: () => SagaStepBuilder,
  SmqlClient: () => SmqlClient,
  SmqlError: () => SmqlError,
  SmqlErrorCode: () => SmqlErrorCode,
  SmqlSubscription: () => SmqlSubscription,
  SpawnBuilder: () => SpawnBuilder,
  SubscriptionError: () => SubscriptionError,
  TimeoutError: () => TimeoutError,
  TrailBuilder: () => TrailBuilder,
  TransitionBuilder: () => TransitionBuilder,
  TransitionDefBuilder: () => TransitionDefBuilder,
  TransitionDeniedError: () => TransitionDeniedError,
  UnauthorizedError: () => UnauthorizedError,
  escapeString: () => escapeString,
  valueToSmql: () => valueToSmql
});
module.exports = __toCommonJS(index_exports);

// src/errors.ts
var SmqlErrorCode = /* @__PURE__ */ ((SmqlErrorCode2) => {
  SmqlErrorCode2["BadRequest"] = "BAD_REQUEST";
  SmqlErrorCode2["NotFound"] = "NOT_FOUND";
  SmqlErrorCode2["TransitionDenied"] = "TRANSITION_DENIED";
  SmqlErrorCode2["Unauthorized"] = "UNAUTHORIZED";
  SmqlErrorCode2["Conflict"] = "CONFLICT";
  SmqlErrorCode2["ServerError"] = "SERVER_ERROR";
  SmqlErrorCode2["Network"] = "NETWORK";
  SmqlErrorCode2["Timeout"] = "TIMEOUT";
  SmqlErrorCode2["Subscription"] = "SUBSCRIPTION";
  return SmqlErrorCode2;
})(SmqlErrorCode || {});
var SmqlError = class extends Error {
  code;
  statusCode;
  constructor(message, code, statusCode) {
    super(message);
    this.name = "SmqlError";
    this.code = code;
    this.statusCode = statusCode;
  }
};
var BadRequestError = class extends SmqlError {
  constructor(message) {
    super(message, "BAD_REQUEST" /* BadRequest */, 400);
    this.name = "BadRequestError";
  }
};
var UnauthorizedError = class extends SmqlError {
  constructor(message) {
    super(message, "UNAUTHORIZED" /* Unauthorized */, 401);
    this.name = "UnauthorizedError";
  }
};
var NotFoundError = class extends SmqlError {
  constructor(message) {
    super(message, "NOT_FOUND" /* NotFound */, 404);
    this.name = "NotFoundError";
  }
};
var TransitionDeniedError = class extends SmqlError {
  constructor(message) {
    super(message, "TRANSITION_DENIED" /* TransitionDenied */, 409);
    this.name = "TransitionDeniedError";
  }
};
var ConflictError = class extends SmqlError {
  constructor(message) {
    super(message, "CONFLICT" /* Conflict */, 409);
    this.name = "ConflictError";
  }
};
var NetworkError = class extends SmqlError {
  constructor(message) {
    super(message, "NETWORK" /* Network */);
    this.name = "NetworkError";
  }
};
var TimeoutError = class extends SmqlError {
  constructor(message) {
    super(message, "TIMEOUT" /* Timeout */);
    this.name = "TimeoutError";
  }
};
var SubscriptionError = class extends SmqlError {
  constructor(message) {
    super(message, "SUBSCRIPTION" /* Subscription */);
    this.name = "SubscriptionError";
  }
};

// src/builder/value.ts
function escapeString(s) {
  return s.replace(/\\/g, "\\\\").replace(/"/g, '\\"');
}
function valueToSmql(val) {
  if (val === null) return "NULL";
  if (typeof val === "boolean") return val ? "true" : "false";
  if (typeof val === "number") return String(val);
  if (typeof val === "string") return `"${escapeString(val)}"`;
  if (Array.isArray(val)) {
    const items = val.map(valueToSmql);
    return `[${items.join(", ")}]`;
  }
  const entries = Object.entries(val);
  if (entries.length === 0) return "{}";
  const fields = entries.map(([k, v]) => `${k}: ${valueToSmql(v)}`);
  return `{${fields.join(", ")}}`;
}
function formatDataFields(data) {
  const entries = Object.entries(data);
  if (entries.length === 0) return "";
  const fields = entries.map(([k, v]) => `${k}: ${valueToSmql(v)}`);
  return ` ${fields.join(", ")} `;
}

// src/builder/spawn.ts
var SpawnBuilder = class {
  machine;
  data = {};
  thenTransition;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  set(keyOrData, value) {
    if (typeof keyOrData === "string") {
      this.data[keyOrData] = value;
    } else {
      Object.assign(this.data, keyOrData);
    }
    return this;
  }
  thenTransitionTo(state) {
    this.thenTransition = state;
    return this;
  }
  toSmql() {
    let s = `SPAWN ${this.machine} {${formatDataFields(this.data)}}`;
    if (this.thenTransition) {
      s += ` THEN TRANSITION TO ${this.thenTransition}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/transition.ts
var TransitionBuilder = class {
  machine;
  instanceId;
  toState;
  isTry;
  withData = [];
  memoText;
  actor;
  throughStates = [];
  orStayFlag = false;
  cascadeFlag = false;
  run;
  constructor(machine, instanceId, toState, isTry, run) {
    this.machine = machine;
    this.instanceId = instanceId;
    this.toState = toState;
    this.isTry = isTry;
    this.run = run;
  }
  with(data) {
    for (const [k, v] of Object.entries(data)) {
      this.withData.push([k, v]);
    }
    return this;
  }
  memo(text) {
    this.memoText = text;
    return this;
  }
  asActor(actor) {
    this.actor = actor;
    return this;
  }
  through(states) {
    this.throughStates = states;
    return this;
  }
  orStay() {
    this.orStayFlag = true;
    return this;
  }
  cascade() {
    this.cascadeFlag = true;
    return this;
  }
  toSmql() {
    let s = `TRANSITION ${this.machine} "${escapeString(this.instanceId)}" TO ${this.toState}`;
    if (this.withData.length > 0) {
      const fields = this.withData.map(
        ([k, v]) => `${k}: ${valueToSmql(v)}`
      );
      s += ` WITH { ${fields.join(", ")} }`;
    }
    if (this.memoText !== void 0) {
      s += ` MEMO "${escapeString(this.memoText)}"`;
    }
    if (this.actor !== void 0) {
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
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/batch-transition.ts
var BatchTransitionBuilder = class {
  machine;
  filterExpr;
  toState;
  withData = [];
  memoText;
  actor;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  to(state) {
    this.toState = state;
    return this;
  }
  with(data) {
    for (const [k, v] of Object.entries(data)) {
      this.withData.push([k, v]);
    }
    return this;
  }
  memo(text) {
    this.memoText = text;
    return this;
  }
  asActor(actor) {
    this.actor = actor;
    return this;
  }
  toSmql() {
    let s = `TRANSITION ALL ${this.machine}`;
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    if (this.toState) {
      s += ` TO ${this.toState}`;
    }
    if (this.withData.length > 0) {
      const fields = this.withData.map(
        ([k, v]) => `${k}: ${valueToSmql(v)}`
      );
      s += ` WITH { ${fields.join(", ")} }`;
    }
    if (this.memoText !== void 0) {
      s += ` MEMO "${escapeString(this.memoText)}"`;
    }
    if (this.actor !== void 0) {
      s += ` AS "${escapeString(this.actor)}"`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/get.ts
var GetBuilder = class {
  machine;
  instanceId;
  actorRole;
  run;
  constructor(machine, instanceId, run) {
    this.machine = machine;
    this.instanceId = instanceId;
    this.run = run;
  }
  asActor(role) {
    this.actorRole = role;
    return this;
  }
  toSmql() {
    let s = `GET ${this.machine} "${escapeString(this.instanceId)}"`;
    if (this.actorRole) {
      s += ` AS ACTOR ${this.actorRole}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/find.ts
var FindBuilder = class {
  machine;
  selectFields = [];
  filterExpr;
  sorts = [];
  limitVal;
  offsetVal;
  afterId;
  actorRole;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  select(...fields) {
    this.selectFields.push(...fields);
    return this;
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  inState(state) {
    this.filterExpr = `STATE IS ${state}`;
    return this;
  }
  stuckIn(state, duration) {
    this.filterExpr = `STUCK IN ${state} FOR ${duration}`;
    return this;
  }
  sortBy(field, direction = "ASC") {
    this.sorts.push({ field, direction });
    return this;
  }
  limit(n) {
    this.limitVal = n;
    return this;
  }
  offset(n) {
    this.offsetVal = n;
    return this;
  }
  after(id) {
    this.afterId = id;
    return this;
  }
  asActor(role) {
    this.actorRole = role;
    return this;
  }
  toSmql() {
    let s = `FIND ${this.machine}`;
    if (this.selectFields.length > 0) {
      s += ` SELECT ${this.selectFields.join(", ")}`;
    }
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    if (this.sorts.length > 0) {
      const clauses = this.sorts.map((c) => `${c.field} ${c.direction}`);
      s += ` SORT BY ${clauses.join(", ")}`;
    }
    if (this.limitVal !== void 0) {
      s += ` LIMIT ${this.limitVal}`;
    }
    if (this.offsetVal !== void 0) {
      s += ` OFFSET ${this.offsetVal}`;
    }
    if (this.afterId !== void 0) {
      s += ` AFTER "${escapeString(this.afterId)}"`;
    }
    if (this.actorRole) {
      s += ` AS ACTOR ${this.actorRole}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
  async first() {
    const prev = this.limitVal;
    this.limitVal = 1;
    const result = await this.run(this.toSmql());
    this.limitVal = prev;
    return result.instances[0] ?? null;
  }
  async count() {
    const result = await this.run(this.toSmql());
    return result.count;
  }
};

// src/builder/aggregate.ts
var AggregateBuilder = class {
  machine;
  measures = [];
  filterExpr;
  groupByClauses = [];
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  measure(func, field, alias) {
    this.measures.push({ func, field, alias });
    return this;
  }
  count(alias) {
    return this.measure("COUNT", void 0, alias);
  }
  sum(field, alias) {
    return this.measure("SUM", field, alias);
  }
  avg(field, alias) {
    return this.measure("AVG", field, alias);
  }
  min(field, alias) {
    return this.measure("MIN", field, alias);
  }
  max(field, alias) {
    return this.measure("MAX", field, alias);
  }
  percentile(field, alias) {
    return this.measure("PERCENTILE", field, alias);
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  groupByState() {
    this.groupByClauses.push("STATE");
    return this;
  }
  groupBy(field) {
    this.groupByClauses.push(field);
    return this;
  }
  toSmql() {
    let s = `AGGREGATE ${this.machine}`;
    if (this.measures.length > 0) {
      const parts = this.measures.map((m) => {
        let clause = `${m.func}(${m.field ?? ""})`;
        if (m.alias) clause += ` AS ${m.alias}`;
        return clause;
      });
      s += ` MEASURE ${parts.join(", ")}`;
    }
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    if (this.groupByClauses.length > 0) {
      s += ` GROUP BY ${this.groupByClauses.join(", ")}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/trail.ts
var TrailBuilder = class {
  instanceId;
  actorFilter;
  fromStateFilter;
  toStateFilter;
  sinceExpr;
  untilExpr;
  run;
  constructor(instanceId, run) {
    this.instanceId = instanceId;
    this.run = run;
  }
  byActor(actor) {
    this.actorFilter = actor;
    return this;
  }
  fromState(state) {
    this.fromStateFilter = state;
    return this;
  }
  toState(state) {
    this.toStateFilter = state;
    return this;
  }
  since(expr) {
    this.sinceExpr = expr;
    return this;
  }
  until(expr) {
    this.untilExpr = expr;
    return this;
  }
  toSmql() {
    let s = `TRAIL OF "${escapeString(this.instanceId)}"`;
    const filters = [];
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
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/paths.ts
var PathsBuilder = class {
  machine;
  filterExpr;
  limitVal;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  limit(n) {
    this.limitVal = n;
    return this;
  }
  toSmql() {
    let s = `PATHS FROM ${this.machine}`;
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    if (this.limitVal !== void 0) {
      s += ` LIMIT ${this.limitVal}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/funnel.ts
var FunnelBuilder = class {
  machine;
  states = [];
  filterExpr;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  through(states) {
    this.states = states;
    return this;
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  toSmql() {
    let s = `FUNNEL ${this.machine} THROUGH [${this.states.join(", ")}]`;
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/compare-paths.ts
var ComparePathsBuilder = class {
  machine;
  segmentField;
  filterExpr;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  segmentBy(field) {
    this.segmentField = field;
    return this;
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  toSmql() {
    let s = `COMPARE PATHS ${this.machine}`;
    if (this.segmentField) {
      s += ` SEGMENT BY ${this.segmentField}`;
    }
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/define-machine.ts
var DefineMachineBuilder = class {
  name;
  dataFields = [];
  stateNames = [];
  initial;
  terminals = [];
  transitions = [];
  children = [];
  parentMachine;
  hooks = [];
  roles = [];
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  data(name, type, ...constraints) {
    this.dataFields.push({ name, type, constraints });
    return this;
  }
  states(...names) {
    this.stateNames.push(...names);
    return this;
  }
  initialState(state) {
    this.initial = state;
    return this;
  }
  terminalStates(...states) {
    this.terminals.push(...states);
    return this;
  }
  transition(from, to) {
    return new TransitionDefBuilder(this, from, to);
  }
  /** @internal */
  _addTransition(entry) {
    this.transitions.push(entry);
    return this;
  }
  child(name, machine, cardinality) {
    this.children.push({ name, machine, cardinality });
    return this;
  }
  parent(machine) {
    this.parentMachine = machine;
    return this;
  }
  hook(trigger) {
    return new HookDefBuilder(this, trigger);
  }
  /** @internal */
  _addHook(entry) {
    this.hooks.push(entry);
    return this;
  }
  role(name) {
    return new RoleDefBuilder(this, name);
  }
  /** @internal */
  _addRole(entry) {
    this.roles.push(entry);
    return this;
  }
  toSmql() {
    const blocks = [];
    if (this.dataFields.length > 0) {
      const fields = this.dataFields.map((f) => {
        let s = `    ${f.name} : ${formatDataType(f.type)}`;
        if (f.constraints.length > 0) {
          s += ` -> ${f.constraints.map(formatConstraint).join(", ")}`;
        }
        return s;
      });
      blocks.push(`  DATA {
${fields.join("\n")}
  }`);
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
        const bodyLines = [];
        for (const g of t.guards) bodyLines.push(`    GUARD: ${g}`);
        for (const a of t.actions) bodyLines.push(`    ACTION: ${a}`);
        for (const m of t.mutates) bodyLines.push(`    MUTATE: ${m}`);
        if (t.timeout) bodyLines.push(`    TIMEOUT: ${t.timeout}`);
        for (const p of t.policies) bodyLines.push(`    APPLY POLICY ${p}`);
        if (t.reactive) bodyLines.push(`    REACTIVE WHEN ${t.reactive}`);
        const body2 = bodyLines.length > 0 ? `
${bodyLines.join("\n")}
  ` : " ";
        return `  ${t.from} -> ${t.to} {${body2}}`;
      });
      blocks.push(`  TRANSITIONS {
${defs.join("\n")}
  }`);
    }
    if (this.children.length > 0) {
      const defs = this.children.map((c) => {
        const card = c.cardinality ?? `LIST(${c.machine})`;
        return `    ${c.name} : ${card}`;
      });
      blocks.push(`  CHILDREN {
${defs.join("\n")}
  }`);
    }
    if (this.parentMachine) {
      blocks.push(`  PARENT: ${this.parentMachine}`);
    }
    if (this.hooks.length > 0) {
      const defs = this.hooks.map((h) => {
        const actions = h.actions.map((a) => `    ${a}`).join("\n");
        return `  ${h.trigger} {
${actions}
  }`;
      });
      blocks.push(`  HOOKS {
${defs.join("\n")}
  }`);
    }
    if (this.roles.length > 0) {
      const defs = this.roles.map((r) => {
        const perms = r.permissions.map((p) => `    ${p}`).join("\n");
        return `  ${r.name} {
${perms}
  }`;
      });
      blocks.push(`  ROLES {
${defs.join("\n")}
  }`);
    }
    const body = blocks.length > 0 ? `
${blocks.join("\n")}
` : "";
    return `DEFINE MACHINE ${this.name} (${body})`;
  }
  execute() {
    return this.run(this.toSmql());
  }
};
var TransitionDefBuilder = class {
  parent;
  entry;
  constructor(parent, from, to) {
    this.parent = parent;
    this.entry = {
      from,
      to,
      guards: [],
      actions: [],
      mutates: [],
      policies: []
    };
  }
  guard(expr) {
    this.entry.guards.push(expr);
    return this;
  }
  action(actionStr) {
    this.entry.actions.push(actionStr);
    return this;
  }
  mutate(field, expr) {
    this.entry.mutates.push(`${field} = ${expr}`);
    return this;
  }
  timeout(duration, targetState) {
    this.entry.timeout = `${duration} -> ${targetState}`;
    return this;
  }
  applyPolicy(name) {
    this.entry.policies.push(name);
    return this;
  }
  reactive(condition) {
    this.entry.reactive = condition;
    return this;
  }
  end() {
    this.parent._addTransition(this.entry);
    return this.parent;
  }
};
var HookDefBuilder = class {
  parent;
  entry;
  constructor(parent, trigger) {
    this.parent = parent;
    this.entry = { trigger, actions: [] };
  }
  action(actionStr) {
    this.entry.actions.push(actionStr);
    return this;
  }
  end() {
    this.parent._addHook(this.entry);
    return this.parent;
  }
};
var RoleDefBuilder = class {
  parent;
  entry;
  constructor(parent, name) {
    this.parent = parent;
    this.entry = { name, permissions: [] };
  }
  canSpawn() {
    this.entry.permissions.push("CAN SPAWN");
    return this;
  }
  canTransition(...states) {
    if (states.length > 0) {
      this.entry.permissions.push(`CAN TRANSITION [${states.join(", ")}]`);
    } else {
      this.entry.permissions.push("CAN TRANSITION");
    }
    return this;
  }
  canQuery() {
    this.entry.permissions.push("CAN QUERY");
    return this;
  }
  canAlter() {
    this.entry.permissions.push("CAN ALTER");
    return this;
  }
  canAll() {
    this.entry.permissions.push("CAN ALL");
    return this;
  }
  canRead(...fields) {
    this.entry.permissions.push(`CAN READ { ${fields.join(", ")} }`);
    return this;
  }
  canWrite(...fields) {
    this.entry.permissions.push(`CAN WRITE { ${fields.join(", ")} }`);
    return this;
  }
  cannotRead(...fields) {
    this.entry.permissions.push(`CANNOT READ { ${fields.join(", ")} }`);
    return this;
  }
  cannotWrite(...fields) {
    this.entry.permissions.push(`CANNOT WRITE { ${fields.join(", ")} }`);
    return this;
  }
  end() {
    this.parent._addRole(this.entry);
    return this.parent;
  }
};
function formatDataType(t) {
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
function formatConstraint(c) {
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

// src/builder/define-policy.ts
var DefinePolicyBuilder = class {
  name;
  guards = [];
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  guard(expr) {
    this.guards.push(expr);
    return this;
  }
  toSmql() {
    const lines = this.guards.map((g) => `  GUARD: ${g}`);
    return `DEFINE POLICY ${this.name} {
${lines.join("\n")}
}`;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/define-view.ts
var DefineViewBuilder = class {
  name;
  machineName;
  filterExpr;
  sorts = [];
  limitVal;
  offsetVal;
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  find(machine) {
    this.machineName = machine;
    return this;
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  sortBy(field, direction = "ASC") {
    this.sorts.push({ field, direction });
    return this;
  }
  limit(n) {
    this.limitVal = n;
    return this;
  }
  offset(n) {
    this.offsetVal = n;
    return this;
  }
  toSmql() {
    let s = `DEFINE VIEW ${this.name} AS FIND ${this.machineName}`;
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    if (this.sorts.length > 0) {
      const clauses = this.sorts.map((c) => `${c.field} ${c.direction}`);
      s += ` SORT BY ${clauses.join(", ")}`;
    }
    if (this.limitVal !== void 0) {
      s += ` LIMIT ${this.limitVal}`;
    }
    if (this.offsetVal !== void 0) {
      s += ` OFFSET ${this.offsetVal}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/define-projection.ts
var DefineProjectionBuilder = class {
  name;
  machineName;
  measures = [];
  filterExpr;
  groupByClauses = [];
  refreshPolicy;
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  aggregate(machine) {
    this.machineName = machine;
    return this;
  }
  measure(func, field, alias) {
    this.measures.push({ func, field, alias });
    return this;
  }
  count(alias) {
    return this.measure("COUNT", void 0, alias);
  }
  sum(field, alias) {
    return this.measure("SUM", field, alias);
  }
  avg(field, alias) {
    return this.measure("AVG", field, alias);
  }
  where(expr) {
    this.filterExpr = expr;
    return this;
  }
  groupByState() {
    this.groupByClauses.push("STATE");
    return this;
  }
  groupBy(field) {
    this.groupByClauses.push(field);
    return this;
  }
  refreshOnTransition() {
    this.refreshPolicy = "REFRESH ON TRANSITION";
    return this;
  }
  refreshOnInterval(seconds) {
    this.refreshPolicy = `REFRESH ON INTERVAL ${seconds}`;
    return this;
  }
  refreshManual() {
    this.refreshPolicy = "REFRESH MANUAL";
    return this;
  }
  toSmql() {
    let s = `DEFINE PROJECTION ${this.name} AS AGGREGATE ${this.machineName}`;
    if (this.measures.length > 0) {
      const parts = this.measures.map((m) => {
        let clause = `${m.func}(${m.field ?? ""})`;
        if (m.alias) clause += ` AS ${m.alias}`;
        return clause;
      });
      s += ` MEASURE ${parts.join(", ")}`;
    }
    if (this.filterExpr) {
      s += ` WHERE ${this.filterExpr}`;
    }
    if (this.groupByClauses.length > 0) {
      s += ` GROUP BY ${this.groupByClauses.join(", ")}`;
    }
    if (this.refreshPolicy) {
      s += ` ${this.refreshPolicy}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/define-rule.ts
var DefineRuleBuilder = class {
  name;
  triggerClause;
  invariantExpr;
  messageText;
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  beforeTransition(machine) {
    this.triggerClause = `BEFORE TRANSITION ON ${machine}`;
    return this;
  }
  beforeSpawn(machine) {
    this.triggerClause = `BEFORE SPAWN ON ${machine}`;
    return this;
  }
  beforeAnyTransition() {
    this.triggerClause = "BEFORE ANY TRANSITION";
    return this;
  }
  afterTransition(machine) {
    this.triggerClause = `AFTER TRANSITION ON ${machine}`;
    return this;
  }
  invariant(expr, message) {
    this.invariantExpr = expr;
    if (message) this.messageText = message;
    return this;
  }
  toSmql() {
    const lines = [];
    if (this.invariantExpr) {
      lines.push(`  INVARIANT: ${this.invariantExpr}`);
    }
    if (this.messageText) {
      lines.push(`  MESSAGE: "${escapeString(this.messageText)}"`);
    }
    return `DEFINE RULE ${this.name} ${this.triggerClause} {
${lines.join("\n")}
}`;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/define-subscription.ts
var DefineSubscriptionBuilder = class {
  name;
  eventClause;
  actions = [];
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  onEnter(state, machine) {
    this.eventClause = `ON ENTER ${state} ON ${machine}`;
    return this;
  }
  onExit(state, machine) {
    this.eventClause = `ON EXIT ${state} ON ${machine}`;
    return this;
  }
  onSpawn(machine) {
    this.eventClause = `ON SPAWN ${machine}`;
    return this;
  }
  onTransition(machine, from, to) {
    let clause = `ON TRANSITION ${machine}`;
    if (from) clause += ` FROM ${from}`;
    if (to) clause += ` TO ${to}`;
    this.eventClause = clause;
    return this;
  }
  action(actionStr) {
    this.actions.push(`ACTION: ${actionStr}`);
    return this;
  }
  actionWhen(condition, actionStr) {
    this.actions.push(`ACTION WHEN ${condition}: ${actionStr}`);
    return this;
  }
  toSmql() {
    const lines = this.actions.map((a) => `  ${a}`);
    return `DEFINE SUBSCRIPTION ${this.name} ${this.eventClause} {
${lines.join("\n")}
}`;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/define-saga.ts
var DefineSagaBuilder = class {
  name;
  triggerClause;
  steps = [];
  onCompleteActions = [];
  onFailureActions = [];
  run;
  constructor(name, run) {
    this.name = name;
    this.run = run;
  }
  triggerOnEnter(state, machine) {
    this.triggerClause = `TRIGGER ON ENTER ${state} ON ${machine}`;
    return this;
  }
  triggerOnSpawn(machine) {
    this.triggerClause = `TRIGGER ON SPAWN ${machine}`;
    return this;
  }
  triggerManual() {
    this.triggerClause = "TRIGGER MANUAL";
    return this;
  }
  step(name) {
    return new SagaStepBuilder(this, name);
  }
  /** @internal */
  _addStep(step) {
    this.steps.push(step);
    return this;
  }
  onComplete(action) {
    this.onCompleteActions.push(action);
    return this;
  }
  onFailure(action) {
    this.onFailureActions.push(action);
    return this;
  }
  toSmql() {
    const lines = [];
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
    return `DEFINE SAGA ${this.name} ${this.triggerClause} {
${lines.join("\n")}
}`;
  }
  execute() {
    return this.run(this.toSmql());
  }
};
var SagaStepBuilder = class {
  parent;
  stepName;
  transitionStr;
  whenExpr;
  compensateStr;
  constructor(parent, name) {
    this.parent = parent;
    this.stepName = name;
  }
  transition(machine, instanceExpr, toState) {
    this.transitionStr = `TRANSITION ${machine} ${instanceExpr} TO ${toState}`;
    return this;
  }
  when(condition) {
    this.whenExpr = condition;
    return this;
  }
  compensate(machine, instanceExpr, toState) {
    this.compensateStr = `${machine} ${instanceExpr} TO ${toState}`;
    return this;
  }
  end() {
    this.parent._addStep({
      name: this.stepName,
      transition: this.transitionStr,
      when: this.whenExpr,
      compensate: this.compensateStr
    });
    return this.parent;
  }
};

// src/builder/alter-machine.ts
var AlterMachineBuilder = class {
  machine;
  operations = [];
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  addState(name) {
    this.operations.push({ toSmql: () => `ADD STATE ${name}` });
    return this;
  }
  removeState(state, migrateTo) {
    this.operations.push({
      toSmql: () => `REMOVE STATE ${state} MIGRATE TO ${migrateTo}`
    });
    return this;
  }
  addTransition(from, to) {
    this.operations.push({
      toSmql: () => `ADD TRANSITION ${from} -> ${to}`
    });
    return this;
  }
  removeTransition(from, to) {
    this.operations.push({
      toSmql: () => `REMOVE TRANSITION ${from} -> ${to}`
    });
    return this;
  }
  addData(field, type, constraints, backfillValue) {
    this.operations.push({
      toSmql: () => {
        let s = `ADD DATA ${field} : ${formatDataType2(type)}`;
        if (constraints && constraints.length > 0) {
          s += ` -> ${constraints.map(formatConstraint2).join(", ")}`;
        }
        if (backfillValue) {
          s += ` BACKFILL ${backfillValue}`;
        }
        return s;
      }
    });
    return this;
  }
  removeData(field) {
    this.operations.push({ toSmql: () => `REMOVE DATA ${field}` });
    return this;
  }
  backfill(field, expr) {
    this.operations.push({
      toSmql: () => `BACKFILL ${field} = ${expr}`
    });
    return this;
  }
  toSmql() {
    const ops = this.operations.map((o) => o.toSmql()).join(" ");
    return `ALTER MACHINE ${this.machine} ${ops}`;
  }
  execute() {
    return this.run(this.toSmql());
  }
};
function formatDataType2(t) {
  if (typeof t === "string") return t;
  switch (t.type) {
    case "ENUM":
      return `ENUM(${t.variants.join(", ")})`;
    case "REF":
      return `REF(${t.target})`;
    case "LIST":
      return `LIST(${formatDataType2(t.inner)})`;
    case "SET":
      return `SET(${formatDataType2(t.inner)})`;
    case "MAP":
      return `MAP(${formatDataType2(t.key)}, ${formatDataType2(t.value)})`;
    case "MONEY":
      return `MONEY(${t.currency})`;
  }
}
function formatConstraint2(c) {
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
      if (typeof c.value === "string") return `DEFAULT("${c.value}")`;
      return `DEFAULT(${c.value})`;
    case "PATTERN":
      return `PATTERN("${c.regex}")`;
    case "COMPUTED":
      return `COMPUTED(${c.expr})`;
  }
}

// src/builder/explain-transitions.ts
var ExplainTransitionsBuilder = class {
  machine;
  instanceId;
  actor;
  run;
  constructor(machine, run) {
    this.machine = machine;
    this.run = run;
  }
  instance(id) {
    this.instanceId = id;
    return this;
  }
  asActor(actor) {
    this.actor = actor;
    return this;
  }
  toSmql() {
    let s = `EXPLAIN TRANSITIONS FOR ${this.machine}`;
    if (this.instanceId) {
      s += ` "${escapeString(this.instanceId)}"`;
    }
    if (this.actor) {
      s += ` AS ${this.actor}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/builder/get-events.ts
var GetEventsBuilder = class {
  machine;
  afterId;
  limitVal;
  run;
  constructor(run, machine) {
    this.machine = machine;
    this.run = run;
  }
  after(id) {
    this.afterId = id;
    return this;
  }
  limit(n) {
    this.limitVal = n;
    return this;
  }
  toSmql() {
    let s = "GET EVENTS";
    if (this.machine) {
      s += ` ${this.machine}`;
    }
    if (this.afterId) {
      s += ` AFTER "${escapeString(this.afterId)}"`;
    }
    if (this.limitVal !== void 0) {
      s += ` LIMIT ${this.limitVal}`;
    }
    return s;
  }
  execute() {
    return this.run(this.toSmql());
  }
};

// src/subscription.ts
var SmqlSubscription = class {
  url;
  token;
  ws = null;
  handlers = /* @__PURE__ */ new Map();
  anyHandlers = /* @__PURE__ */ new Set();
  _connected = false;
  constructor(baseUrl, options) {
    const wsUrl = baseUrl.replace(/^http:/, "ws:").replace(/^https:/, "wss:");
    let url = `${wsUrl}/subscribe`;
    const params = [];
    if (options?.machine) params.push(`machine=${encodeURIComponent(options.machine)}`);
    if (options?.event) params.push(`event=${encodeURIComponent(options.event)}`);
    if (options?.token) {
      this.token = options.token;
      params.push(`token=${encodeURIComponent(options.token)}`);
    }
    if (params.length > 0) url += `?${params.join("&")}`;
    this.url = url;
  }
  get connected() {
    return this._connected;
  }
  connect() {
    return new Promise((resolve, reject) => {
      try {
        this.ws = new WebSocket(this.url);
      } catch (err) {
        reject(new SubscriptionError(`Failed to create WebSocket: ${err}`));
        return;
      }
      this.ws.onopen = () => {
        this._connected = true;
        resolve();
      };
      this.ws.onerror = (event) => {
        if (!this._connected) {
          reject(new SubscriptionError("WebSocket connection failed"));
        }
      };
      this.ws.onclose = () => {
        this._connected = false;
      };
      this.ws.onmessage = (event) => {
        try {
          const data = JSON.parse(
            typeof event.data === "string" ? event.data : ""
          );
          this.dispatch(data);
        } catch {
        }
      };
    });
  }
  on(event, handler) {
    let set = this.handlers.get(event);
    if (!set) {
      set = /* @__PURE__ */ new Set();
      this.handlers.set(event, set);
    }
    set.add(handler);
    return () => {
      set.delete(handler);
    };
  }
  onAny(handler) {
    this.anyHandlers.add(handler);
    return () => {
      this.anyHandlers.delete(handler);
    };
  }
  close() {
    if (this.ws) {
      this.ws.close();
      this.ws = null;
      this._connected = false;
    }
  }
  dispatch(event) {
    const handlers = this.handlers.get(event.event);
    if (handlers) {
      for (const h of handlers) h(event);
    }
    for (const h of this.anyHandlers) h(event);
  }
};

// src/client.ts
var SmqlClient = class {
  baseUrl;
  token;
  timeout;
  headers;
  constructor(config) {
    this.baseUrl = config.url.replace(/\/+$/, "");
    this.token = config.token;
    this.timeout = config.timeout ?? 3e4;
    this.headers = config.headers ?? {};
  }
  // --- Internal request helper ---
  async request(method, path, body) {
    const url = `${this.baseUrl}${path}`;
    const headers = {
      ...this.headers
    };
    if (body !== void 0) {
      headers["Content-Type"] = "application/json";
    }
    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);
    let response;
    try {
      response = await fetch(url, {
        method,
        headers,
        body: body !== void 0 ? JSON.stringify(body) : void 0,
        signal: controller.signal
      });
    } catch (err) {
      clearTimeout(timer);
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new TimeoutError(`Request timed out after ${this.timeout}ms`);
      }
      const message = err instanceof Error ? err.message : String(err);
      throw new NetworkError(`Network error: ${message}`);
    } finally {
      clearTimeout(timer);
    }
    if (!response.ok) {
      let errorMsg;
      try {
        const errBody = await response.json();
        errorMsg = errBody.error ?? response.statusText;
      } catch {
        errorMsg = response.statusText;
      }
      switch (response.status) {
        case 400:
          throw new BadRequestError(errorMsg);
        case 401:
          throw new UnauthorizedError(errorMsg);
        case 404:
          throw new NotFoundError(errorMsg);
        case 409:
          throw new TransitionDeniedError(errorMsg);
        default:
          throw new SmqlError(
            errorMsg,
            "SERVER_ERROR" /* ServerError */,
            response.status
          );
      }
    }
    return await response.json();
  }
  // --- Raw execute ---
  async execute(smql) {
    const url = `${this.baseUrl}/execute`;
    const headers = {
      "Content-Type": "application/json",
      ...this.headers
    };
    if (this.token) {
      headers["Authorization"] = `Bearer ${this.token}`;
    }
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);
    let response;
    try {
      response = await fetch(url, {
        method: "POST",
        headers,
        body: JSON.stringify({ smql }),
        signal: controller.signal
      });
    } catch (err) {
      clearTimeout(timer);
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new TimeoutError(`Request timed out after ${this.timeout}ms`);
      }
      const message = err instanceof Error ? err.message : String(err);
      throw new NetworkError(`Network error: ${message}`);
    } finally {
      clearTimeout(timer);
    }
    const body = await response.json();
    if (!body.success) {
      const msg = body.error ?? "Unknown error";
      switch (response.status) {
        case 400:
          throw new BadRequestError(msg);
        case 401:
          throw new UnauthorizedError(msg);
        case 404:
          throw new NotFoundError(msg);
        case 409:
          throw new TransitionDeniedError(msg);
        default:
          throw new SmqlError(msg, "SERVER_ERROR" /* ServerError */, response.status);
      }
    }
    return body;
  }
  async executeAs(smql) {
    const resp = await this.execute(smql);
    if (resp.result === void 0) {
      throw new SmqlError("No result in response", "SERVER_ERROR" /* ServerError */);
    }
    return resp.result;
  }
  // --- REST wrappers ---
  async health() {
    const body = await this.request("GET", "/health");
    return body.status === "ok";
  }
  async listMachines() {
    const body = await this.request("GET", "/machines");
    return body.machines;
  }
  async getMachine(name) {
    return this.request("GET", `/machines/${encodeURIComponent(name)}`);
  }
  async getInstance(id) {
    return this.request("GET", `/instances/${encodeURIComponent(id)}`);
  }
  async deleteInstance(id) {
    return this.request(
      "DELETE",
      `/instances/${encodeURIComponent(id)}`
    );
  }
  async getMetrics() {
    const url = `${this.baseUrl}/metrics`;
    const headers = { ...this.headers };
    if (this.token) headers["Authorization"] = `Bearer ${this.token}`;
    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), this.timeout);
    let response;
    try {
      response = await fetch(url, { headers, signal: controller.signal });
    } catch (err) {
      clearTimeout(timer);
      if (err instanceof DOMException && err.name === "AbortError") {
        throw new TimeoutError(`Request timed out after ${this.timeout}ms`);
      }
      const message = err instanceof Error ? err.message : String(err);
      throw new NetworkError(`Network error: ${message}`);
    } finally {
      clearTimeout(timer);
    }
    return response.text();
  }
  // --- Builder entry points ---
  spawn(machine) {
    return new SpawnBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  transition(machine, id, state) {
    return new TransitionBuilder(
      machine,
      id,
      state,
      false,
      (smql) => this.executeAs(smql)
    );
  }
  tryTransition(machine, id, state) {
    return new TransitionBuilder(
      machine,
      id,
      state,
      true,
      (smql) => this.executeAs(smql)
    );
  }
  transitionAll(machine) {
    return new BatchTransitionBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  get(machine, id) {
    return new GetBuilder(
      machine,
      id,
      (smql) => this.executeAs(smql)
    );
  }
  find(machine) {
    return new FindBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  aggregate(machine) {
    return new AggregateBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  trail(id) {
    return new TrailBuilder(
      id,
      (smql) => this.executeAs(smql)
    );
  }
  paths(machine) {
    return new PathsBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  funnel(machine) {
    return new FunnelBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  comparePaths(machine) {
    return new ComparePathsBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  explainTransitions(machine) {
    return new ExplainTransitionsBuilder(
      machine,
      (smql) => this.executeAs(smql)
    );
  }
  getEvents(machine) {
    return new GetEventsBuilder(
      (smql) => this.executeAs(smql),
      machine
    );
  }
  async getTransitions(id, actor) {
    let path = `/instances/${encodeURIComponent(id)}/transitions`;
    if (actor) {
      path += `?as=${encodeURIComponent(actor)}`;
    }
    return this.request("GET", path);
  }
  defineMachine(name) {
    return new DefineMachineBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  definePolicy(name) {
    return new DefinePolicyBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  defineView(name) {
    return new DefineViewBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  defineProjection(name) {
    return new DefineProjectionBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  defineRule(name) {
    return new DefineRuleBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  defineSubscription(name) {
    return new DefineSubscriptionBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  defineSaga(name) {
    return new DefineSagaBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  alterMachine(name) {
    return new AlterMachineBuilder(
      name,
      (smql) => this.executeAs(smql)
    );
  }
  async getView(name) {
    return this.executeAs(`GET VIEW ${name}`);
  }
  async getProjection(name) {
    return this.executeAs(`GET PROJECTION ${name}`);
  }
  subscribe(options) {
    return new SmqlSubscription(this.baseUrl, {
      ...options,
      token: this.token
    });
  }
};

// src/builder/expression.ts
var Expr = class _Expr {
  expr;
  constructor(expr) {
    this.expr = expr;
  }
  toString() {
    return this.expr;
  }
  // --- Factories ---
  static field(name) {
    return new _Expr(name);
  }
  static val(v) {
    return new _Expr(valueToSmql(v));
  }
  static stateIs(state) {
    return new _Expr(`STATE IS ${state}`);
  }
  static stateIn(...states) {
    return new _Expr(`STATE IN { ${states.join(", ")} }`);
  }
  static isSet(field) {
    return new _Expr(`${field} IS SET`);
  }
  static isNotSet(field) {
    return new _Expr(`${field} IS NOT SET`);
  }
  static raw(expr) {
    return new _Expr(expr);
  }
  // --- Query predicates ---
  static alive() {
    return new _Expr("ALIVE");
  }
  static terminated() {
    return new _Expr("TERMINATED");
  }
  static stuckIn(state, duration) {
    return new _Expr(`STUCK_IN("${state}", ${duration})`);
  }
  static hasVisited(state) {
    return new _Expr(`HAS_VISITED("${state}")`);
  }
  static neverVisited(state) {
    return new _Expr(`NEVER_VISITED("${state}")`);
  }
  static tag(key, value) {
    return new _Expr(`TAG "${key}" == "${value}"`);
  }
  // --- Composition predicates ---
  static parentState() {
    return new _Expr("PARENT.STATE");
  }
  static parentField(field) {
    return new _Expr(`PARENT.${field}`);
  }
  static signalFrom(machine, condition) {
    return new _Expr(`SIGNAL FROM ${machine} WHERE ${condition}`);
  }
  static all(collection, predicate) {
    return new _Expr(`ALL(${collection}, ${predicate})`);
  }
  static any(collection, predicate) {
    return new _Expr(`ANY(${collection}, ${predicate})`);
  }
  static countOf(collection) {
    return new _Expr(`COUNT(${collection})`);
  }
  // --- Built-in functions ---
  static elapsed() {
    return new _Expr("elapsed()");
  }
  static elapsedSince(state) {
    return new _Expr(`elapsed_since("${state}")`);
  }
  static now() {
    return new _Expr("NOW()");
  }
  static today() {
    return new _Expr("TODAY()");
  }
  static timeoutRemaining() {
    return new _Expr("timeout_remaining()");
  }
  static len(expr) {
    return new _Expr(`len(${expr})`);
  }
  static lower(expr) {
    return new _Expr(`lower(${expr})`);
  }
  static upper(expr) {
    return new _Expr(`upper(${expr})`);
  }
  static pattern(regex) {
    return new _Expr(`PATTERN("${regex}")`);
  }
  // --- Comparisons ---
  eq(other) {
    return new _Expr(`${this.expr} == ${toExprStr(other)}`);
  }
  neq(other) {
    return new _Expr(`${this.expr} != ${toExprStr(other)}`);
  }
  gt(other) {
    return new _Expr(`${this.expr} > ${toExprStr(other)}`);
  }
  gte(other) {
    return new _Expr(`${this.expr} >= ${toExprStr(other)}`);
  }
  lt(other) {
    return new _Expr(`${this.expr} < ${toExprStr(other)}`);
  }
  lte(other) {
    return new _Expr(`${this.expr} <= ${toExprStr(other)}`);
  }
  // --- Logical ---
  and(other) {
    return new _Expr(`(${this.expr}) AND (${other.expr})`);
  }
  or(other) {
    return new _Expr(`(${this.expr}) OR (${other.expr})`);
  }
  not() {
    return new _Expr(`NOT (${this.expr})`);
  }
  // --- Arithmetic ---
  add(other) {
    return new _Expr(`${this.expr} + ${toExprStr(other)}`);
  }
  sub(other) {
    return new _Expr(`${this.expr} - ${toExprStr(other)}`);
  }
  mul(other) {
    return new _Expr(`${this.expr} * ${toExprStr(other)}`);
  }
  div(other) {
    return new _Expr(`${this.expr} / ${toExprStr(other)}`);
  }
  // --- Field access ---
  dot(field) {
    return new _Expr(`${this.expr}.${field}`);
  }
  // --- Set membership ---
  in(...values) {
    const items = values.map(valueToSmql);
    return new _Expr(`${this.expr} IN { ${items.join(", ")} }`);
  }
};
function toExprStr(v) {
  if (v instanceof Expr) return v.toString();
  return valueToSmql(v);
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  AggregateBuilder,
  AlterMachineBuilder,
  BadRequestError,
  BatchTransitionBuilder,
  ComparePathsBuilder,
  ConflictError,
  DefineMachineBuilder,
  DefinePolicyBuilder,
  DefineProjectionBuilder,
  DefineRuleBuilder,
  DefineSagaBuilder,
  DefineSubscriptionBuilder,
  DefineViewBuilder,
  ExplainTransitionsBuilder,
  Expr,
  FindBuilder,
  FunnelBuilder,
  GetBuilder,
  GetEventsBuilder,
  HookDefBuilder,
  NetworkError,
  NotFoundError,
  PathsBuilder,
  RoleDefBuilder,
  SagaStepBuilder,
  SmqlClient,
  SmqlError,
  SmqlErrorCode,
  SmqlSubscription,
  SpawnBuilder,
  SubscriptionError,
  TimeoutError,
  TrailBuilder,
  TransitionBuilder,
  TransitionDefBuilder,
  TransitionDeniedError,
  UnauthorizedError,
  escapeString,
  valueToSmql
});
//# sourceMappingURL=index.cjs.map