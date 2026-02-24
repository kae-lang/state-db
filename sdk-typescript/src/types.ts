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
