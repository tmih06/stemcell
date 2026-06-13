//! Repository Module
//!
//! Repository pattern implementations for database access.

pub mod channel_message;
pub mod cron_job;
pub mod cron_job_run;
pub mod feedback_ledger;
pub mod file;
pub mod kg_pending_batch;
pub mod knowledge_graph;
pub mod message;
pub mod pending_request;
pub mod plan;
pub mod recent_paths;
pub mod session;
pub mod tool_execution;
pub mod usage_ledger;

pub use channel_message::{ChannelMessageRepository, TopicSummary};
pub use cron_job::CronJobRepository;
pub use cron_job_run::CronJobRunRepository;
pub use feedback_ledger::FeedbackLedgerRepository;
pub use file::FileRepository;
pub use kg_pending_batch::{KgBatchStats, KgPendingBatch, KgPendingBatchRepository};
pub use knowledge_graph::{
    KnowledgeGraphRepository, LinkDirection, Neighbor, NoteRecord, NoteUpsert, ObservationInput,
    ObservationRecord, RelationInput, SearchHit,
};
pub use message::MessageRepository;
pub use pending_request::PendingRequestRepository;
pub use plan::PlanRepository;
pub use recent_paths::RecentPathsRepository;
pub use session::{SessionListOptions, SessionRepository};
pub use tool_execution::ToolExecutionRepository;
pub use usage_ledger::UsageLedgerRepository;
