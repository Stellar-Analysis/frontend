//! Ingestion watermark tagging for point-in-time consistent backups.
//!
//! Every backup is tagged with the last-ingested ledger sequence recorded at
//! snapshot time. Because the datastore and cache do not share a transaction
//! boundary, their snapshots may be taken a moment apart; the watermark lets a
//! restore detect and reconcile that skew instead of silently combining
//! inconsistent state.

/// The ingestion watermark: the last ledger sequence ingested at snapshot time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Watermark(pub u64);

impl Watermark {
    pub fn new(sequence: u64) -> Self {
        Watermark(sequence)
    }

    pub fn sequence(&self) -> u64 {
        self.0
    }

    /// Reconcile this watermark with another, allowing a documented tolerance.
    ///
    /// Returns the lower of the two watermarks (the safe resume point) when the
    /// difference is within `tolerance`, otherwise a [`WatermarkMismatch`].
    pub fn reconcile(
        &self,
        other: &Watermark,
        tolerance: u64,
    ) -> Result<Watermark, WatermarkMismatch> {
        let diff = self.0.abs_diff(other.0);
        if diff <= tolerance {
            Ok(Watermark(self.0.min(other.0)))
        } else {
            Err(WatermarkMismatch {
                datastore: *self,
                cache: *other,
                tolerance,
            })
        }
    }
}

/// The datastore and cache watermarks disagree beyond the documented tolerance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatermarkMismatch {
    pub datastore: Watermark,
    pub cache: Watermark,
    pub tolerance: u64,
}

impl std::fmt::Display for WatermarkMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "datastore watermark {} and cache watermark {} differ by more than tolerance {}",
            self.datastore.0,
            self.cache.0,
            self.tolerance
        )
    }
}

impl std::error::Error for WatermarkMismatch {}
