use serde::{Deserialize, Serialize};

/// Reason why a worker was terminated
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TerminationReason {
    /// Worker completed successfully
    Success,

    /// Worker exceeded CPU time limit
    CpuTimeLimit,

    /// Worker exceeded wall-clock time limit
    WallClockTimeout,

    /// Worker exceeded memory limit (heap or ArrayBuffer)
    MemoryLimit,

    /// Worker threw an uncaught exception
    Exception,

    /// Worker failed to initialize
    InitializationError,

    /// Worker was terminated by external signal
    Terminated,
}

impl TerminationReason {
    /// Returns true if this represents a successful completion
    pub fn is_success(&self) -> bool {
        matches!(self, Self::Success)
    }

    /// Returns true if this represents a resource limit violation
    pub fn is_limit_exceeded(&self) -> bool {
        matches!(
            self,
            Self::CpuTimeLimit | Self::WallClockTimeout | Self::MemoryLimit
        )
    }

    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::Success => "Worker completed successfully",
            Self::CpuTimeLimit => "Worker exceeded CPU time limit",
            Self::WallClockTimeout => "Worker exceeded wall-clock time limit",
            Self::MemoryLimit => "Worker exceeded memory limit",
            Self::Exception => "Worker threw an uncaught exception",
            Self::InitializationError => "Worker failed to initialize",
            Self::Terminated => "Worker was terminated",
        }
    }

    /// Get an appropriate HTTP status code for this termination reason
    pub fn http_status(&self) -> u16 {
        match self {
            Self::Success => 200,
            Self::CpuTimeLimit | Self::MemoryLimit => 429, // Too Many Requests
            Self::WallClockTimeout => 504, // Gateway Timeout
            Self::Exception | Self::InitializationError => 500, // Internal Server Error
            Self::Terminated => 503, // Service Unavailable
        }
    }
}

impl std::fmt::Display for TerminationReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.description())
    }
}
