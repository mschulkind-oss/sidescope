//! Shared, source-independent chart contracts: bounded decimals and time, the
//! configuration shape, the validated dataset, reductions, geometry and the
//! per-source state every chart lane builds on.

pub mod config;
pub mod dataset;
pub mod decimal;
pub mod error;
pub mod facts;
pub mod geometry;
pub mod reduce;
pub mod state;
pub mod time;
