//! The recording session: every M3 track recorder driven as one recording (REC-1..REC-8).
//!
//! * [`session`]: [`RecordingSession`], the orchestrator (bundle, clock, start/pause/resume/stop,
//!   partial failure, disk guard, live status).
//! * [`tracks`]: [`SessionTrack`] and one wrapper per recorder crate.

pub mod engine;
pub mod params;
pub mod selftest;
pub mod session;
pub mod sleep;
pub mod sources;
pub mod tracks;

pub use engine::{
    clock_text, EngineOptions, JobHolds, Phase, PreparedSelftest, PreparedStart, RecordingEngine,
    DEFAULT_EXCLUDED_BUNDLE_IDS, PAUSE_UPLOADS_SETTING,
};
pub use params::{
    AudioPreset, CameraParams, ExcludeParams, MicParams, RecordMode, ResolvedStart, StartParams,
};
pub use selftest::{Fingerprint, Fps60Rejection, Fps60Verdict, SelftestEntry, SelftestParams};
pub use session::{
    LevelReading, RecordingSession, SessionConfig, SessionNote, SessionState, SessionStatus,
    StoppedSession, TrackState, TrackStatus,
};
pub use sleep::{FakeIdleSleep, IdleSleepInhibitor, SystemIdleSleep};
pub use sources::{
    BuildContext, DeviceNames, Preflight, PreflightWarning, SensorRequest, SourceFactory,
    SystemSources,
};
pub use tracks::{
    CameraTrack, CursorTrack, MicTrack, NoteLevel, PlainTrack, ScreenTrack, SessionTrack,
    SystemAudioTrack, TrackNote,
};
