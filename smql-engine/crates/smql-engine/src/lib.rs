// SMQL Engine Core — State machine execution engine

pub mod engine;
pub mod eval;
pub mod query;

#[cfg(test)]
mod tests;

pub use engine::{Engine, SpawnResult, TransitionResult};
pub use eval::{eval_expr, eval_guard, ActorInfo, EvalContext};
pub use query::{
    AggregateRow, FunnelResult, FunnelStage, PathResult, QueryResult,
};
