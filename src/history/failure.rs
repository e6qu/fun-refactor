use super::{Action, History};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FailurePhase {
    Preparation,
    Installation,
    Recovery,
    Finalization,
}

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum FailureOutcome {
    Unchanged,
    RolledBack,
    RecoveryIncomplete,
    FinalizationUncertain,
}

#[derive(Debug, Serialize)]
pub struct HistoryFailure {
    pub transaction: u64,
    pub action: Action,
    pub phase: FailurePhase,
    pub outcome: FailureOutcome,
    pub journal_pending: Option<bool>,
    pub recovery_error: Option<String>,
    #[serde(skip)]
    cause: anyhow::Error,
}

impl HistoryFailure {
    pub(super) fn new(
        history: &History,
        transaction: u64,
        action: Action,
        phase: FailurePhase,
        outcome: FailureOutcome,
        cause: anyhow::Error,
        recovery_error: Option<String>,
    ) -> Self {
        let journal_pending = History::read(&history.root)
            .ok()
            .map(|history| history.pending.is_some());
        Self {
            transaction,
            action,
            phase,
            outcome,
            journal_pending,
            recovery_error,
            cause,
        }
    }
}

impl std::fmt::Display for HistoryFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "transaction {} {:?} failed during {:?}: {}",
            self.transaction, self.action, self.phase, self.cause
        )?;
        match self.outcome {
            FailureOutcome::Unchanged => write!(f, "; source installation did not start")?,
            FailureOutcome::RolledBack => write!(f, "; restored its starting state")?,
            FailureOutcome::RecoveryIncomplete => write!(f, "; recovery did not complete")?,
            FailureOutcome::FinalizationUncertain => write!(
                f,
                "; changed source but could not confirm durable finalization."
            )?,
        }
        if let Some(error) = &self.recovery_error {
            write!(f, "; recovery error: {error}")?;
        }
        match self.journal_pending {
            Some(true) => write!(f, "; run `fr history recover {} --write`", self.transaction),
            Some(false) => write!(
                f,
                "; journal has no pending transaction; inspect `fr history` before retrying."
            ),
            None => write!(
                f,
                "; journal state is unknown; inspect `fr history` before retrying."
            ),
        }
    }
}

impl std::error::Error for HistoryFailure {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.cause.as_ref())
    }
}
