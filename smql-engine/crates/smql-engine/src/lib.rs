// SMQL Engine Core — State machine execution engine

pub mod alter;
pub mod engine;
pub mod eval;
pub mod query;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod eval_tests;

pub use alter::AlterResult;
pub use engine::{Engine, SpawnResult, TransitionResult};
pub use eval::{eval_expr, eval_guard, ActorInfo, ChildInfo, EvalContext};
pub use query::{
    AggregateRow, FunnelResult, FunnelStage, PathResult, QueryResult,
};
