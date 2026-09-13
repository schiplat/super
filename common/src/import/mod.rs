//! Import program stacks from foreign process-manager config files.
//!
//! Parsing lives in `common` so the CLI and tests share one implementation;
//! the daemon is not involved (import goes through the existing stack API).
//!
//! Each format implements [`StackFormat`], producing a [`StackDraft`] of
//! service requests plus structured [`ImportWarning`]s for everything that
//! cannot be carried over verbatim. Warnings are the deliverable: they let
//! the user review exactly what changed before anything is applied.

use std::path::PathBuf;

use crate::CreateProgramRequest;

pub mod supervisor;
pub use supervisor::SupervisorFormat;

/// How much of a source construct survived the mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WarningSeverity {
    /// Carried over, but the user should verify (e.g. value came from the
    /// current shell environment).
    Info,
    /// Dropped, approximated, or likely rejected — needs user attention.
    Warn,
}

/// A single mapping note produced while converting a source config.
#[derive(Debug, Clone)]
pub struct ImportWarning {
    pub severity: WarningSeverity,
    /// Source location, e.g. `[program:worker]`.
    pub section: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// The output of parsing a foreign config: mapped services + review notes.
#[derive(Debug, Clone, Default)]
pub struct StackDraft {
    pub services: Vec<CreateProgramRequest>,
    pub warnings: Vec<ImportWarning>,
}

/// Context threaded into format parsers.
#[derive(Debug, Clone, Default)]
pub struct ParseCtx {
    /// Directory of the source file (used for `%(here)s`-style expansion and
    /// as the base for relative include paths).
    pub here_dir: Option<PathBuf>,
    /// Whether `[include]`-style recursion may read other files.
    pub allow_include: bool,
    /// Rewrite custom log file paths to plain file names so they land in the
    /// target log dir (source daemons often point at their own spool dirs,
    /// which the target daemon rejects for security reasons).
    pub remap_logs: bool,
}

/// A converter from one foreign config format to [`StackDraft`].
pub trait StackFormat: Sync {
    /// Registry id, e.g. `"supervisor"`.
    fn id(&self) -> &'static str;
    /// Cheap content sniff so obviously-wrong inputs fail with a friendly
    /// error instead of a cryptic parse error.
    fn detect(&self, input: &str) -> bool;
    fn parse(&self, input: &str, ctx: &ParseCtx) -> anyhow::Result<StackDraft>;
}

/// Look up a registered format by id.
pub fn stack_format_by_id(id: &str) -> Option<&'static dyn StackFormat> {
    match id {
        supervisor::FORMAT_ID => Some(&SupervisorFormat),
        _ => None,
    }
}

/// All ids accepted by `super import` (for CLI help and error messages).
pub fn supported_format_ids() -> &'static [&'static str] {
    &[supervisor::FORMAT_ID]
}
