//! Queries, one module per group of tables. Every query is parameterised;
//! nothing is built by concatenating input into SQL.

pub mod devices;
pub mod flows;
pub mod keys;
pub mod links;
pub mod sync;
pub mod users;
