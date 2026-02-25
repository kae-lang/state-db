interface SmqlClientConfig {
    url: string;
    token?: string;
    timeout?: number;
    headers?: Record<string, string>;
}
interface ExecuteResponse<T = unknown> {
    success: boolean;
    result?: T;
    error?: string;
    warnings?: string[];
}
interface Instance {
    id: string;
    machine: string;
    state: string;
    data: Record<string, unknown>;
    created_at: string;
    updated_at: string;
    state_entered_at: string;
    trail_length: number;
    version: number;
}
interface DefineResult {
    action: string;
    name?: string;
}
type SpawnResult = Instance;
interface TransitionResult {
    from_state: string;
    to_state: string;
    instance: Instance;
}
type TryTransitionResult = {
    transitioned: true;
    from_state: string;
    to_state: string;
    instance: Instance;
} | {
    transitioned: false;
};
interface BatchTransitionResult {
    action: string;
    machine: string;
    matched: number;
    transitioned: number;
    failed: number;
    failures: {
        instance_id: string;
        error: string;
    }[];
}
interface AlterMachineResult {
    action: string;
    machine: string;
    new_version: number;
    operations_applied: number;
    instances_migrated: number;
}
interface FindResult {
    count: number;
    instances: Instance[];
    next_cursor?: string;
}
interface TrailEntry {
    sequence: number;
    from_state: string;
    to_state: string;
    actor?: string | null;
    memo?: string | null;
    timestamp: string;
}
interface TrailResult {
    count: number;
    entries: TrailEntry[];
}
interface AggregateRow {
    group: Record<string, unknown>;
    measures: Record<string, unknown>;
}
interface AggregateResult {
    rows: AggregateRow[];
}
interface PathEntry {
    path: string[];
    count: number;
}
interface PathsResult {
    paths: PathEntry[];
}
interface FunnelStage {
    state: string;
    count: number;
    conversion_rate: number;
}
interface FunnelResult {
    stages: FunnelStage[];
}
interface ComparePathsSegment {
    segment_value: unknown;
    paths: PathEntry[];
}
interface ComparePathsResult {
    segment_by: string;
    segments: ComparePathsSegment[];
}
interface ExplainedTransition {
    from_state: string;
    to_state: string;
    guards: string[];
    guards_met: boolean;
    blocking_guards: string[];
    recovery_options: RecoveryOption[];
    requires_data: string[];
    requires_role: string | null;
}
interface ExplainTransitionsResult {
    machine: string;
    current_state?: string;
    instance_id?: string;
    transitions: ExplainedTransition[];
}
interface StoredEvent {
    id: string;
    timestamp: string;
    machine: string;
    event_name: string;
    instance_id?: string;
    payload?: unknown;
    actor?: string;
}
interface EventsResult {
    count: number;
    events: StoredEvent[];
    next_cursor?: string;
}
/**
 * Action types for AI agent recovery from errors.
 */
type RecoveryAction = "SET_FIELD" | "CHANGE_ACTOR" | "ESCALATE" | "RETRY" | "WAIT";
/**
 * An actionable recovery option for AI agents.
 */
interface RecoveryOption {
    /** The type of recovery action. */
    action: RecoveryAction;
    /** The field to set (for SET_FIELD action). */
    field?: string;
    /** A suggested value or description of what to provide. */
    suggested_value?: string;
    /** Why this option would help resolve the error. */
    reason: string;
    /** An example SMQL command showing how to apply this recovery. */
    example?: string;
}
interface HealthResponse {
    status: string;
}
interface MachinesListResponse {
    machines: string[];
}
interface MachineInfo {
    name: string;
    states: string[];
    initial_state: string;
    terminal_states: string[];
    version: number;
}
interface DeleteInstanceResult {
    deleted: boolean;
    id: string;
}
interface SubscriptionEvent {
    event: string;
    machine: string;
    instance_id?: string;
    actor?: string;
    data?: unknown;
}
interface SubscribeOptions {
    machine?: string;
    event?: string;
}
type SmqlValue = null | boolean | number | string | SmqlValue[] | {
    [key: string]: SmqlValue;
};
type SortDirection = "ASC" | "DESC";
interface SortClause {
    field: string;
    direction: SortDirection;
}
type DataType = "TEXT" | "INT" | "FLOAT" | "BOOL" | "UUID" | "DATE" | "DATETIME" | "DURATION" | "BLOB" | "JSON" | {
    type: "ENUM";
    variants: string[];
} | {
    type: "REF";
    target: string;
} | {
    type: "LIST";
    inner: DataType;
} | {
    type: "SET";
    inner: DataType;
} | {
    type: "MAP";
    key: DataType;
    value: DataType;
} | {
    type: "MONEY";
    currency: string;
};
type Constraint = "REQUIRED" | "OPTIONAL" | "UNIQUE" | {
    type: "MAX";
    value: number;
} | {
    type: "MIN";
    value: number;
} | {
    type: "RANGE";
    lo: number;
    hi: number;
} | {
    type: "DEFAULT";
    value: string | number | boolean | null;
} | {
    type: "PATTERN";
    regex: string;
} | {
    type: "COMPUTED";
    expr: string;
};

type RunFn$j = (smql: string) => Promise<Instance>;
declare class SpawnBuilder {
    private machine;
    private data;
    private thenTransition?;
    private run;
    constructor(machine: string, run: RunFn$j);
    set(data: Record<string, SmqlValue>): this;
    set(key: string, value: SmqlValue): this;
    thenTransitionTo(state: string): this;
    toSmql(): string;
    execute(): Promise<Instance>;
}

type RunFn$i<T> = (smql: string) => Promise<T>;
declare class TransitionBuilder<T = TransitionResult> {
    private machine;
    private instanceId;
    private toState;
    private isTry;
    private withData;
    private memoText?;
    private actor?;
    private throughStates;
    private orStayFlag;
    private cascadeFlag;
    private run;
    constructor(machine: string, instanceId: string, toState: string, isTry: boolean, run: RunFn$i<T>);
    with(data: Record<string, SmqlValue>): this;
    memo(text: string): this;
    asActor(actor: string): this;
    through(states: string[]): this;
    orStay(): this;
    cascade(): this;
    toSmql(): string;
    execute(): Promise<T>;
}

type RunFn$h = (smql: string) => Promise<BatchTransitionResult>;
declare class BatchTransitionBuilder {
    private machine;
    private filterExpr?;
    private toState?;
    private withData;
    private memoText?;
    private actor?;
    private run;
    constructor(machine: string, run: RunFn$h);
    where(expr: string): this;
    to(state: string): this;
    with(data: Record<string, SmqlValue>): this;
    memo(text: string): this;
    asActor(actor: string): this;
    toSmql(): string;
    execute(): Promise<BatchTransitionResult>;
}

type RunFn$g = (smql: string) => Promise<Instance>;
declare class GetBuilder {
    private machine;
    private instanceId;
    private actorRole?;
    private run;
    constructor(machine: string, instanceId: string, run: RunFn$g);
    asActor(role: string): this;
    toSmql(): string;
    execute(): Promise<Instance>;
}

type RunFn$f = (smql: string) => Promise<FindResult>;
declare class FindBuilder {
    private machine;
    private selectFields;
    private filterExpr?;
    private sorts;
    private limitVal?;
    private offsetVal?;
    private afterId?;
    private actorRole?;
    private run;
    constructor(machine: string, run: RunFn$f);
    select(...fields: string[]): this;
    where(expr: string): this;
    inState(state: string): this;
    stuckIn(state: string, duration: string): this;
    sortBy(field: string, direction?: SortDirection): this;
    limit(n: number): this;
    offset(n: number): this;
    after(id: string): this;
    asActor(role: string): this;
    toSmql(): string;
    execute(): Promise<FindResult>;
    first(): Promise<Instance | null>;
    count(): Promise<number>;
}

type RunFn$e = (smql: string) => Promise<AggregateResult>;
declare class AggregateBuilder {
    private machine;
    private measures;
    private filterExpr?;
    private groupByClauses;
    private run;
    constructor(machine: string, run: RunFn$e);
    measure(func: string, field?: string, alias?: string): this;
    count(alias?: string): this;
    sum(field: string, alias?: string): this;
    avg(field: string, alias?: string): this;
    min(field: string, alias?: string): this;
    max(field: string, alias?: string): this;
    percentile(field: string, alias?: string): this;
    where(expr: string): this;
    groupByState(): this;
    groupBy(field: string): this;
    toSmql(): string;
    execute(): Promise<AggregateResult>;
}

type RunFn$d = (smql: string) => Promise<TrailResult>;
declare class TrailBuilder {
    private instanceId;
    private actorFilter?;
    private fromStateFilter?;
    private toStateFilter?;
    private sinceExpr?;
    private untilExpr?;
    private run;
    constructor(instanceId: string, run: RunFn$d);
    byActor(actor: string): this;
    fromState(state: string): this;
    toState(state: string): this;
    since(expr: string): this;
    until(expr: string): this;
    toSmql(): string;
    execute(): Promise<TrailResult>;
}

type RunFn$c = (smql: string) => Promise<PathsResult>;
declare class PathsBuilder {
    private machine;
    private filterExpr?;
    private limitVal?;
    private run;
    constructor(machine: string, run: RunFn$c);
    where(expr: string): this;
    limit(n: number): this;
    toSmql(): string;
    execute(): Promise<PathsResult>;
}

type RunFn$b = (smql: string) => Promise<FunnelResult>;
declare class FunnelBuilder {
    private machine;
    private states;
    private filterExpr?;
    private run;
    constructor(machine: string, run: RunFn$b);
    through(states: string[]): this;
    where(expr: string): this;
    toSmql(): string;
    execute(): Promise<FunnelResult>;
}

type RunFn$a = (smql: string) => Promise<ComparePathsResult>;
declare class ComparePathsBuilder {
    private machine;
    private segmentField?;
    private filterExpr?;
    private run;
    constructor(machine: string, run: RunFn$a);
    segmentBy(field: string): this;
    where(expr: string): this;
    toSmql(): string;
    execute(): Promise<ComparePathsResult>;
}

type RunFn$9 = (smql: string) => Promise<DefineResult>;
declare class DefineMachineBuilder {
    private name;
    private dataFields;
    private stateNames;
    private initial?;
    private terminals;
    private transitions;
    private children;
    private parentMachine?;
    private hooks;
    private roles;
    private run;
    constructor(name: string, run: RunFn$9);
    data(name: string, type: DataType, ...constraints: Constraint[]): this;
    states(...names: string[]): this;
    initialState(state: string): this;
    terminalStates(...states: string[]): this;
    transition(from: string, to: string): TransitionDefBuilder;
    /** @internal */
    _addTransition(entry: TransitionDefEntry): this;
    child(name: string, machine: string, cardinality?: string): this;
    parent(machine: string): this;
    hook(trigger: string): HookDefBuilder;
    /** @internal */
    _addHook(entry: HookDef): this;
    role(name: string): RoleDefBuilder;
    /** @internal */
    _addRole(entry: RoleDef): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}
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
declare class TransitionDefBuilder {
    private parent;
    private entry;
    constructor(parent: DefineMachineBuilder, from: string, to: string);
    guard(expr: string): this;
    action(actionStr: string): this;
    mutate(field: string, expr: string): this;
    timeout(duration: string, targetState: string): this;
    applyPolicy(name: string): this;
    reactive(condition: string): this;
    end(): DefineMachineBuilder;
}
interface HookDef {
    trigger: string;
    actions: string[];
}
declare class HookDefBuilder {
    private parent;
    private entry;
    constructor(parent: DefineMachineBuilder, trigger: string);
    action(actionStr: string): this;
    end(): DefineMachineBuilder;
}
interface RoleDef {
    name: string;
    permissions: string[];
}
declare class RoleDefBuilder {
    private parent;
    private entry;
    constructor(parent: DefineMachineBuilder, name: string);
    canSpawn(): this;
    canTransition(...states: string[]): this;
    canQuery(): this;
    canAlter(): this;
    canAll(): this;
    canRead(...fields: string[]): this;
    canWrite(...fields: string[]): this;
    cannotRead(...fields: string[]): this;
    cannotWrite(...fields: string[]): this;
    end(): DefineMachineBuilder;
}

type RunFn$8 = (smql: string) => Promise<DefineResult>;
declare class DefinePolicyBuilder {
    private name;
    private guards;
    private run;
    constructor(name: string, run: RunFn$8);
    guard(expr: string): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}

type RunFn$7 = (smql: string) => Promise<DefineResult>;
declare class DefineViewBuilder {
    private name;
    private machineName?;
    private filterExpr?;
    private sorts;
    private limitVal?;
    private offsetVal?;
    private run;
    constructor(name: string, run: RunFn$7);
    find(machine: string): this;
    where(expr: string): this;
    sortBy(field: string, direction?: SortDirection): this;
    limit(n: number): this;
    offset(n: number): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}

type RunFn$6 = (smql: string) => Promise<DefineResult>;
declare class DefineProjectionBuilder {
    private name;
    private machineName?;
    private measures;
    private filterExpr?;
    private groupByClauses;
    private refreshPolicy?;
    private run;
    constructor(name: string, run: RunFn$6);
    aggregate(machine: string): this;
    measure(func: string, field?: string, alias?: string): this;
    count(alias?: string): this;
    sum(field: string, alias?: string): this;
    avg(field: string, alias?: string): this;
    where(expr: string): this;
    groupByState(): this;
    groupBy(field: string): this;
    refreshOnTransition(): this;
    refreshOnInterval(seconds: number): this;
    refreshManual(): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}

type RunFn$5 = (smql: string) => Promise<DefineResult>;
declare class DefineRuleBuilder {
    private name;
    private triggerClause?;
    private invariantExpr?;
    private messageText?;
    private run;
    constructor(name: string, run: RunFn$5);
    beforeTransition(machine: string): this;
    beforeSpawn(machine: string): this;
    beforeAnyTransition(): this;
    afterTransition(machine: string): this;
    invariant(expr: string, message?: string): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}

type RunFn$4 = (smql: string) => Promise<DefineResult>;
declare class DefineSubscriptionBuilder {
    private name;
    private eventClause?;
    private actions;
    private run;
    constructor(name: string, run: RunFn$4);
    onEnter(state: string, machine: string): this;
    onExit(state: string, machine: string): this;
    onSpawn(machine: string): this;
    onTransition(machine: string, from?: string, to?: string): this;
    action(actionStr: string): this;
    actionWhen(condition: string, actionStr: string): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}

type RunFn$3 = (smql: string) => Promise<DefineResult>;
interface SagaStepDef {
    name: string;
    transition: string;
    when?: string;
    compensate?: string;
}
declare class DefineSagaBuilder {
    private name;
    private triggerClause?;
    private steps;
    private onCompleteActions;
    private onFailureActions;
    private run;
    constructor(name: string, run: RunFn$3);
    triggerOnEnter(state: string, machine: string): this;
    triggerOnSpawn(machine: string): this;
    triggerManual(): this;
    step(name: string): SagaStepBuilder;
    /** @internal */
    _addStep(step: SagaStepDef): this;
    onComplete(action: string): this;
    onFailure(action: string): this;
    toSmql(): string;
    execute(): Promise<DefineResult>;
}
declare class SagaStepBuilder {
    private parent;
    private stepName;
    private transitionStr?;
    private whenExpr?;
    private compensateStr?;
    constructor(parent: DefineSagaBuilder, name: string);
    transition(machine: string, instanceExpr: string, toState: string): this;
    when(condition: string): this;
    compensate(machine: string, instanceExpr: string, toState: string): this;
    end(): DefineSagaBuilder;
}

type RunFn$2 = (smql: string) => Promise<AlterMachineResult>;
declare class AlterMachineBuilder {
    private machine;
    private operations;
    private run;
    constructor(machine: string, run: RunFn$2);
    addState(name: string): this;
    removeState(state: string, migrateTo: string): this;
    addTransition(from: string, to: string): this;
    removeTransition(from: string, to: string): this;
    addData(field: string, type: DataType, constraints?: Constraint[], backfillValue?: string): this;
    removeData(field: string): this;
    backfill(field: string, expr: string): this;
    toSmql(): string;
    execute(): Promise<AlterMachineResult>;
}

type RunFn$1 = (smql: string) => Promise<ExplainTransitionsResult>;
declare class ExplainTransitionsBuilder {
    private machine;
    private instanceId?;
    private actor?;
    private run;
    constructor(machine: string, run: RunFn$1);
    instance(id: string): this;
    asActor(actor: string): this;
    toSmql(): string;
    execute(): Promise<ExplainTransitionsResult>;
}

type RunFn = (smql: string) => Promise<EventsResult>;
declare class GetEventsBuilder {
    private machine?;
    private afterId?;
    private limitVal?;
    private run;
    constructor(run: RunFn, machine?: string);
    after(id: string): this;
    limit(n: number): this;
    toSmql(): string;
    execute(): Promise<EventsResult>;
}

type EventHandler = (event: SubscriptionEvent) => void;
declare class SmqlSubscription {
    private url;
    private token?;
    private ws;
    private handlers;
    private anyHandlers;
    private _connected;
    constructor(baseUrl: string, options?: SubscribeOptions & {
        token?: string;
    });
    get connected(): boolean;
    connect(): Promise<void>;
    on(event: string, handler: EventHandler): () => void;
    onAny(handler: EventHandler): () => void;
    close(): void;
    private dispatch;
}

declare class SmqlClient {
    private baseUrl;
    private token?;
    private timeout;
    private headers;
    constructor(config: SmqlClientConfig);
    private request;
    execute(smql: string): Promise<ExecuteResponse>;
    executeAs<T>(smql: string): Promise<T>;
    health(): Promise<boolean>;
    listMachines(): Promise<string[]>;
    getMachine(name: string): Promise<MachineInfo>;
    getInstance(id: string): Promise<Instance>;
    deleteInstance(id: string): Promise<DeleteInstanceResult>;
    getMetrics(): Promise<string>;
    spawn(machine: string): SpawnBuilder;
    transition(machine: string, id: string, state: string): TransitionBuilder<TransitionResult>;
    tryTransition(machine: string, id: string, state: string): TransitionBuilder<TryTransitionResult>;
    transitionAll(machine: string): BatchTransitionBuilder;
    get(machine: string, id: string): GetBuilder;
    find(machine: string): FindBuilder;
    aggregate(machine: string): AggregateBuilder;
    trail(id: string): TrailBuilder;
    paths(machine: string): PathsBuilder;
    funnel(machine: string): FunnelBuilder;
    comparePaths(machine: string): ComparePathsBuilder;
    explainTransitions(machine: string): ExplainTransitionsBuilder;
    getEvents(machine?: string): GetEventsBuilder;
    getTransitions(id: string, actor?: string): Promise<ExplainTransitionsResult>;
    defineMachine(name: string): DefineMachineBuilder;
    definePolicy(name: string): DefinePolicyBuilder;
    defineView(name: string): DefineViewBuilder;
    defineProjection(name: string): DefineProjectionBuilder;
    defineRule(name: string): DefineRuleBuilder;
    defineSubscription(name: string): DefineSubscriptionBuilder;
    defineSaga(name: string): DefineSagaBuilder;
    alterMachine(name: string): AlterMachineBuilder;
    getView(name: string): Promise<FindResult>;
    getProjection(name: string): Promise<AggregateResult>;
    subscribe(options?: SubscribeOptions): SmqlSubscription;
}

declare enum SmqlErrorCode {
    BadRequest = "BAD_REQUEST",
    NotFound = "NOT_FOUND",
    TransitionDenied = "TRANSITION_DENIED",
    Unauthorized = "UNAUTHORIZED",
    Conflict = "CONFLICT",
    ServerError = "SERVER_ERROR",
    Network = "NETWORK",
    Timeout = "TIMEOUT",
    Subscription = "SUBSCRIPTION"
}
declare class SmqlError extends Error {
    readonly code: SmqlErrorCode;
    readonly statusCode?: number;
    constructor(message: string, code: SmqlErrorCode, statusCode?: number);
}
declare class BadRequestError extends SmqlError {
    constructor(message: string);
}
declare class UnauthorizedError extends SmqlError {
    constructor(message: string);
}
declare class NotFoundError extends SmqlError {
    constructor(message: string);
}
declare class TransitionDeniedError extends SmqlError {
    constructor(message: string);
}
declare class ConflictError extends SmqlError {
    constructor(message: string);
}
declare class NetworkError extends SmqlError {
    constructor(message: string);
}
declare class TimeoutError extends SmqlError {
    constructor(message: string);
}
declare class SubscriptionError extends SmqlError {
    constructor(message: string);
}

declare class Expr {
    private readonly expr;
    private constructor();
    toString(): string;
    static field(name: string): Expr;
    static val(v: SmqlValue): Expr;
    static stateIs(state: string): Expr;
    static stateIn(...states: string[]): Expr;
    static isSet(field: string): Expr;
    static isNotSet(field: string): Expr;
    static raw(expr: string): Expr;
    static alive(): Expr;
    static terminated(): Expr;
    static stuckIn(state: string, duration: string): Expr;
    static hasVisited(state: string): Expr;
    static neverVisited(state: string): Expr;
    static tag(key: string, value: string): Expr;
    static parentState(): Expr;
    static parentField(field: string): Expr;
    static signalFrom(machine: string, condition: string): Expr;
    static all(collection: string, predicate: string): Expr;
    static any(collection: string, predicate: string): Expr;
    static countOf(collection: string): Expr;
    static elapsed(): Expr;
    static elapsedSince(state: string): Expr;
    static now(): Expr;
    static today(): Expr;
    static timeoutRemaining(): Expr;
    static len(expr: string): Expr;
    static lower(expr: string): Expr;
    static upper(expr: string): Expr;
    static pattern(regex: string): Expr;
    eq(other: Expr | SmqlValue): Expr;
    neq(other: Expr | SmqlValue): Expr;
    gt(other: Expr | SmqlValue): Expr;
    gte(other: Expr | SmqlValue): Expr;
    lt(other: Expr | SmqlValue): Expr;
    lte(other: Expr | SmqlValue): Expr;
    and(other: Expr): Expr;
    or(other: Expr): Expr;
    not(): Expr;
    add(other: Expr | SmqlValue): Expr;
    sub(other: Expr | SmqlValue): Expr;
    mul(other: Expr | SmqlValue): Expr;
    div(other: Expr | SmqlValue): Expr;
    dot(field: string): Expr;
    in(...values: SmqlValue[]): Expr;
}

declare function escapeString(s: string): string;
declare function valueToSmql(val: SmqlValue): string;

export { AggregateBuilder, type AggregateResult, type AggregateRow, AlterMachineBuilder, type AlterMachineResult, BadRequestError, BatchTransitionBuilder, type BatchTransitionResult, ComparePathsBuilder, type ComparePathsResult, type ComparePathsSegment, ConflictError, type Constraint, type DataType, DefineMachineBuilder, DefinePolicyBuilder, DefineProjectionBuilder, type DefineResult, DefineRuleBuilder, DefineSagaBuilder, DefineSubscriptionBuilder, DefineViewBuilder, type DeleteInstanceResult, type EventsResult, type ExecuteResponse, ExplainTransitionsBuilder, type ExplainTransitionsResult, type ExplainedTransition, Expr, FindBuilder, type FindResult, FunnelBuilder, type FunnelResult, type FunnelStage, GetBuilder, GetEventsBuilder, type HealthResponse, HookDefBuilder, type Instance, type MachineInfo, type MachinesListResponse, NetworkError, NotFoundError, type PathEntry, PathsBuilder, type PathsResult, RoleDefBuilder, SagaStepBuilder, SmqlClient, type SmqlClientConfig, SmqlError, SmqlErrorCode, SmqlSubscription, type SmqlValue, type SortClause, type SortDirection, SpawnBuilder, type SpawnResult, type StoredEvent, type SubscribeOptions, SubscriptionError, type SubscriptionEvent, TimeoutError, TrailBuilder, type TrailEntry, type TrailResult, TransitionBuilder, TransitionDefBuilder, TransitionDeniedError, type TransitionResult, type TryTransitionResult, UnauthorizedError, escapeString, valueToSmql };
