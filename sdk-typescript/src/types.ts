// --- Client config ---

export interface SmqlClientConfig {
  url: string;
  token?: string;
  timeout?: number;
  headers?: Record<string, string>;
}

// --- Core wrapper ---

export interface ExecuteResponse<T = unknown> {
  success: boolean;
  result?: T;
  error?: string;
  warnings?: string[];
}

// --- Instance model ---

export interface Instance {
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

// --- Command results ---

export interface DefineResult {
  action: string;
  name?: string;
}

export type SpawnResult = Instance;

export interface TransitionResult {
  from_state: string;
  to_state: string;
  instance: Instance;
}

export type TryTransitionResult =
  | { transitioned: true; from_state: string; to_state: string; instance: Instance }
  | { transitioned: false };

export interface BatchTransitionResult {
  action: string;
  machine: string;
  matched: number;
  transitioned: number;
  failed: number;
  failures: { instance_id: string; error: string }[];
}

export interface AlterMachineResult {
  action: string;
  machine: string;
  new_version: number;
  operations_applied: number;
  instances_migrated: number;
}

// --- Query results ---

export interface FindResult {
  count: number;
  instances: Instance[];
  next_cursor?: string;
}

export interface TrailEntry {
  sequence: number;
  from_state: string;
  to_state: string;
  actor?: string | null;
  memo?: string | null;
  timestamp: string;
}

export interface TrailResult {
  count: number;
  entries: TrailEntry[];
}

export interface AggregateRow {
  group: Record<string, unknown>;
  measures: Record<string, unknown>;
}

export interface AggregateResult {
  rows: AggregateRow[];
}

export interface PathEntry {
  path: string[];
  count: number;
}

export interface PathsResult {
  paths: PathEntry[];
}

export interface FunnelStage {
  state: string;
  count: number;
  conversion_rate: number;
}

export interface FunnelResult {
  stages: FunnelStage[];
}

export interface ComparePathsSegment {
  segment_value: unknown;
  paths: PathEntry[];
}

export interface ComparePathsResult {
  segment_by: string;
  segments: ComparePathsSegment[];
}

// --- Explain transitions ---

export interface ExplainedTransition {
  from_state: string;
  to_state: string;
  guards: string[];
  guards_met: boolean;
  blocking_guards: string[];
  recovery_options: RecoveryOption[];
  requires_data: string[];
  requires_role: string | null;
}

export interface ExplainTransitionsResult {
  machine: string;
  current_state?: string;
  instance_id?: string;
  transitions: ExplainedTransition[];
}

// --- Events ---

export interface StoredEvent {
  id: string;
  timestamp: string;
  machine: string;
  event_name: string;
  instance_id?: string;
  payload?: unknown;
  actor?: string;
}

export interface EventsResult {
  count: number;
  events: StoredEvent[];
  next_cursor?: string;
}

// --- Error types for AI agents ---

/**
 * Action types for AI agent recovery from errors.
 */
export type RecoveryAction =
  | "SET_FIELD"
  | "CHANGE_ACTOR"
  | "ESCALATE"
  | "RETRY"
  | "WAIT";

/**
 * An actionable recovery option for AI agents.
 */
export interface RecoveryOption {
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

/**
 * A single guard failure with context.
 */
export interface GuardFailure {
  /** The guard expression that failed. */
  guard_expr: string;
  /** The actual value encountered. */
  actual_value?: string;
  /** The expected value or condition. */
  expected?: string;
  /** A hint for resolving the failure. */
  hint?: string;
}

/**
 * Structured error for denied transitions, including all guard failures
 * and AI-agent-friendly recovery options.
 */
export interface TransitionDeniedError {
  /** The instance ID that failed to transition. */
  instance_id: string;
  /** The state the instance is currently in. */
  from_state: string;
  /** The state the transition attempted to reach. */
  to_state: string;
  /** All guard failures that caused the denial. */
  guard_failures: GuardFailure[];
  /** A general hint for resolving the error. */
  hint?: string;
  /** Actionable recovery options for AI agents. */
  recovery_options: RecoveryOption[];
  /** A human-readable prompt suitable for LLM context. */
  llm_prompt?: string;
}

// --- REST endpoints ---

export interface HealthResponse {
  status: string;
}

export interface MachinesListResponse {
  machines: string[];
}

export interface MachineInfo {
  name: string;
  states: string[];
  initial_state: string;
  terminal_states: string[];
  version: number;
}

export interface DeleteInstanceResult {
  deleted: boolean;
  id: string;
}

// --- WebSocket ---

export interface SubscriptionEvent {
  event: string;
  machine: string;
  instance_id?: string;
  actor?: string;
  data?: unknown;
}

export interface SubscribeOptions {
  machine?: string;
  event?: string;
}

// --- Builder helpers ---

export type SmqlValue =
  | null
  | boolean
  | number
  | string
  | SmqlValue[]
  | { [key: string]: SmqlValue };

export type SortDirection = "ASC" | "DESC";

export interface SortClause {
  field: string;
  direction: SortDirection;
}

export type DataType =
  | "TEXT"
  | "INT"
  | "FLOAT"
  | "BOOL"
  | "UUID"
  | "DATE"
  | "DATETIME"
  | "DURATION"
  | "BLOB"
  | "JSON"
  | { type: "ENUM"; variants: string[] }
  | { type: "REF"; target: string }
  | { type: "LIST"; inner: DataType }
  | { type: "SET"; inner: DataType }
  | { type: "MAP"; key: DataType; value: DataType }
  | { type: "MONEY"; currency: string };

export type Constraint =
  | "REQUIRED"
  | "OPTIONAL"
  | "UNIQUE"
  | { type: "MAX"; value: number }
  | { type: "MIN"; value: number }
  | { type: "RANGE"; lo: number; hi: number }
  | { type: "DEFAULT"; value: string | number | boolean | null }
  | { type: "PATTERN"; regex: string }
  | { type: "COMPUTED"; expr: string };
