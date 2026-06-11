//! Database Layer
//!
//! Provides database connection management, models, and repositories.

mod database;
pub mod models;
pub mod repository;

pub use database::{Database, Pool, PoolExt, db_integrity_failed, global_pool, interact_err};
pub use models::*;
pub use repository::*;
