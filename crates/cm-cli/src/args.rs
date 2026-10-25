//! Command-line grammar (`clap` derive). Each MCP tool has the same name as its subcommand
//! (`capture_screenshot` is `capture screenshot`, `list_library` is `library list`, ...;
//! AGT-1).

use std::path::PathBuf;

use clap::{ArgGroup, Args, Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "capturemann",
    version,
    about = "CaptureMann command-line client (talks to the CaptureMann daemon)",
    after_help = "Exit codes: 0 success, 1 other failure, 2 bad input or not found, \
3 user declined a confirmation, 4 permission missing, 5 platform or network error, \
6 job failed or busy."
)]
pub struct Cli {
    /// Machine-readable JSON on stdout (errors as JSON on stderr).
    #[arg(long, global = true)]
    pub json: bool,
    /// Daemon socket path (Unix) or pipe name (Windows); default: the platform endpoint.
    #[arg(long, global = true, env = "CAPTUREMANN_SOCKET", value_name = "PATH")]
    pub socket: Option<String>,
    /// Do not start a daemon when none is running; fail instead.
    #[arg(long, global = true)]
    pub no_start: bool,
    /// Idempotency key for mutating commands (a retry with the same key replays the result).
    #[arg(long, global = true, value_name = "ID")]
    pub request_id: Option<String>,
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// List displays and windows (`list_targets`).
    Targets {
        /// Only list displays (skips the window enumeration, which is much faster).
        #[arg(long)]
        displays_only: bool,
        #[command(subcommand)]
        cmd: Option<TargetsCmd>,
    },
    /// Screenshots (`capture_screenshot`, `get_capture`).
    Capture {
        #[command(subcommand)]
        cmd: CaptureCmd,
    },
    /// Add annotation ops to a screenshot (`annotate`).
    Annotate(AnnotateArgs),
    /// Undo the last revision of a capture (`undo`).
    Undo(RevisionArgs),
    /// Redo the last undone revision of a capture (`redo`).
    Redo(RevisionArgs),
    /// Render a recording to one combined MP4 with the FFmpeg sidecar (`export_video`). Starts a
    /// background job (`jobs get <id>`); `--wait` blocks until it ends.
    Export(ExportArgs),
    /// Browse the library (`list_library`).
    Library {
        #[command(subcommand)]
        cmd: LibraryCmd,
    },
    /// Read and change settings (`get_settings`, `set_settings`).
    Settings {
        #[command(subcommand)]
        cmd: SettingsCmd,
    },
    /// Screen recording: start, pause, resume, stop, status (`start_recording`, ...).
    Record {
        #[command(subcommand)]
        cmd: RecordCmd,
    },
    /// List cameras, microphones and system-audio support (`list_devices`).
    Devices,
    /// List the destinations set up in Settings (`list_destinations`).
    Destinations,
    /// Copy a capture or a file to a destination in the background (`upload`). Prints the job id.
    Upload(UploadArgs),
    /// Publish a recording to YouTube (`publish_video`), or list the connected accounts
    /// (`publish accounts`). The app asks you to approve every publish; nothing is sent before
    /// you do. Prints the job id.
    Publish(PublishArgs),
    /// Run the MCP server (the `cm-mcp` binary next to this one): stdio by default, local HTTP
    /// with `--http`. Stdout is the MCP channel, so nothing else is printed there.
    Mcp {
        /// Serve Streamable HTTP on 127.0.0.1 with a bearer token instead of stdio.
        #[arg(long)]
        http: bool,
        /// Port for `--http` (default: a random free port).
        #[arg(long, requires = "http", value_name = "N")]
        port: Option<u16>,
    },
    /// Manage the background daemon.
    Daemon {
        #[command(subcommand)]
        cmd: DaemonCmd,
    },
    /// Edit a recording without touching its files: trim, cut, split, speed, background
    /// (`trim`, `cut_segments`, `restore_segments`, `split`, `set_speed`, `set_background`,
    /// `set_audio_mix`).
    /// Times are seconds (`12.5`) or ticks (`t:1500000`); each command is one undo step.
    Edit {
        #[command(subcommand)]
        cmd: EditCmd,
    },
    /// Local AI: speech models, transcripts, captions and suggestions. Everything runs on this
    /// computer; a model is downloaded only when you run `ai models download`.
    Ai {
        #[command(subcommand)]
        cmd: AiCmd,
    },
    /// Background jobs (`get_job`, `list_jobs`, `cancel_job`, `job.*` events).
    Jobs {
        #[command(subcommand)]
        cmd: JobsCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum AiCmd {
    /// List the speech models, or manage them: `models download|import|use|rm`
    /// (`list_models`, `download_model`, `import_model`, `select_model`, `remove_model`).
    Models {
        #[command(subcommand)]
        cmd: Option<ModelsCmd>,
    },
    /// Transcribe a recording with an installed model (`transcribe`); without a model it tells you
    /// how to get one (exit 2, `no_model`).
    Transcribe(TranscribeArgs),
    /// Print a recording's transcript and caption lines (`get_transcript`).
    Transcript {
        /// Capture id of a recording.
        id: String,
        /// Print the words with their times, not only the caption lines.
        #[arg(long)]
        words: bool,
    },
    /// Edit the caption lines (`set_captions`).
    Captions {
        #[command(subcommand)]
        cmd: CaptionsCmd,
    },
    /// Suggestions only, nothing is applied (`suggest_zooms`, `suggest_cuts`).
    Suggest {
        #[command(subcommand)]
        cmd: SuggestCmd,
    },
}

#[derive(Debug, Subcommand)]
pub enum ModelsCmd {
    /// Download a model from the internet (asks you to confirm in the app). Resumes if it was
    /// interrupted.
    Download {
        /// Model id, for example `small` (see `capturemann ai models`).
        model: String,
        /// Wait for the download to finish (exit 6 if it fails).
        #[arg(long)]
        wait: bool,
        /// With `--wait`: give up after this many seconds (the download keeps running).
        #[arg(long, value_name = "SECS", requires = "wait", value_parser = positive_secs)]
        timeout: Option<f64>,
    },
    /// Install a model from a file you already have (no network).
    Import {
        model: String,
        /// The model file, for example `ggml-small.bin`.
        path: PathBuf,
    },
    /// Use this installed model by default.
    Use { model: String },
    /// Delete an installed model (asks you to confirm in the app).
    Rm { model: String },
}

#[derive(Debug, Args)]
pub struct TranscribeArgs {
    /// Capture id of a recording with a microphone or system-audio track.
    pub id: String,
    /// `auto` (detect, the default) or a language code such as `en`, `vi` or `es`.
    #[arg(long, value_name = "LANG")]
    pub language: Option<String>,
    /// Installed model id (default: the active one).
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// Wait for the transcription to finish (exit 6 if it fails).
    #[arg(long)]
    pub wait: bool,
    /// With `--wait`: give up after this many seconds (the job keeps running).
    #[arg(long, value_name = "SECS", requires = "wait", value_parser = positive_secs)]
    pub timeout: Option<f64>,
}

#[derive(Debug, Subcommand)]
pub enum CaptionsCmd {
    /// Print the caption lines: the edited ones, else lines built from the transcript
    /// (`get_captions`).
    Get {
        /// Capture id of a recording.
        id: String,
    },
    /// Replace the caption lines from a JSON file, `-` for stdin: an array of
    /// `{ "start": 1.5, "end": 3, "text": "..." }` (seconds or `t:<ticks>`; at most 2 rows of 42
    /// characters and 7 s per line). One undo step.
    #[command(group(ArgGroup::new("src").required(true).args(["file", "clear"])))]
    Set {
        /// Capture id of a recording.
        id: String,
        /// JSON file with the lines (`-` reads stdin).
        #[arg(long, value_name = "FILE")]
        file: Option<String>,
        /// Remove the edited lines (the generated ones come back).
        #[arg(long)]
        clear: bool,
    },
    /// Find and replace text across all caption lines (`replace_in_captions`): the same matching
    /// as the editor's find (Unicode, Vietnamese diacritics exact, NFC and NFD equal). Give
    /// `--find` and `--replace`, or a JSON file of pairs. One undo step; the original transcript
    /// is not changed.
    #[command(group(ArgGroup::new("pairs").required(true).args(["find", "file"])))]
    Replace {
        /// Capture id of a recording.
        id: String,
        /// Text to find.
        #[arg(long, value_name = "TEXT", requires = "replace")]
        find: Option<String>,
        /// Replacement text (may be empty to delete the match).
        #[arg(long, value_name = "TEXT", requires = "find")]
        replace: Option<String>,
        /// Match upper and lower case exactly.
        #[arg(long, requires = "find")]
        match_case: bool,
        /// Match whole words only.
        #[arg(long, requires = "find")]
        whole_word: bool,
        /// JSON file with the pairs (`-` reads stdin): an array of
        /// `{ "find": "...", "replace": "...", "match_case": false, "whole_word": false }`.
        #[arg(long, value_name = "FILE", conflicts_with = "find")]
        file: Option<String>,
    },
    /// Show or hide the captions on the video, and choose whether exports burn them in by
    /// default (`set_caption_options`). Give at least one option. One undo step.
    #[command(group(ArgGroup::new("what").required(true).multiple(true).args(["show", "hide", "burn", "no_burn"])))]
    Options {
        /// Capture id of a recording.
        id: String,
        /// Show the captions in the editor's preview.
        #[arg(long, conflicts_with = "hide")]
        show: bool,
        /// Hide the captions in the editor's preview.
        #[arg(long)]
        hide: bool,
        /// Burn the captions into exported videos by default.
        #[arg(long, conflicts_with = "no_burn")]
        burn: bool,
        /// Do not burn the captions into exported videos by default.
        #[arg(long = "no-burn")]
        no_burn: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum SuggestCmd {
    /// Zoom regions from clicks and cursor activity; apply with `edit zoom`.
    Zooms {
        /// Capture id of a recording.
        id: String,
    },
    /// Silences and filler words to cut; apply with `edit cut`.
    Cuts {
        /// Capture id of a recording.
        id: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum TargetsCmd {
    /// Same as `capturemann targets`.
    List,
}

#[derive(Debug, Subcommand)]
pub enum CaptureCmd {
    /// Take a screenshot of a display, a window or a region.
    Screenshot(ScreenshotArgs),
    /// Show a capture's metadata and optionally save its current image.
    Get(GetArgs),
}

#[derive(Debug, Args)]
#[command(group(ArgGroup::new("what").required(true).multiple(true).args(["display", "window", "region"])))]
pub struct ScreenshotArgs {
    /// Display id (see `targets`).
    #[arg(long, value_name = "ID", conflicts_with = "window")]
    pub display: Option<u32>,
    /// Window id, or a case-insensitive substring of its title or app name.
    #[arg(long, value_name = "ID|TITLE", conflicts_with_all = ["display", "region"])]
    pub window: Option<String>,
    /// Region `x,y,w,h` in physical px, relative to the display (default: primary display).
    #[arg(long, value_name = "X,Y,W,H", allow_hyphen_values = true)]
    pub region: Option<String>,
    /// Wait this many seconds (0 to 30) before capturing.
    #[arg(long, value_name = "SECONDS")]
    pub delay: Option<f64>,
    /// Also copy the image to the clipboard.
    #[arg(long)]
    pub copy: bool,
    /// Annotation ops to apply right away: a JSON file, inline JSON, or `-` for stdin.
    #[arg(long, value_name = "OPS")]
    pub annotate: Option<String>,
    /// Write the resulting PNG to this file.
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
    /// Library title (default: `Screenshot yyyy-MM-dd HH.mm`).
    #[arg(long)]
    pub title: Option<String>,
}

#[derive(Debug, Args)]
pub struct GetArgs {
    /// Capture id.
    pub id: String,
    /// Revision (only the current one is supported).
    #[arg(long)]
    pub revision: Option<u64>,
    /// Write the current image (annotations applied) to this PNG file.
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct AnnotateArgs {
    /// Capture id.
    pub id: String,
    /// Ops as a JSON array (or `{"ops": [...]}`): inline JSON, a file path, or `-` for stdin.
    #[arg(
        long,
        value_name = "JSON|FILE|-",
        required_unless_present_any = ["suggest_redactions", "redact_emails"],
        conflicts_with_all = ["suggest_redactions", "redact_emails"]
    )]
    pub ops: Option<String>,
    /// Replace the existing ops instead of appending (still one undo step).
    #[arg(long, conflicts_with_all = ["suggest_redactions", "redact_emails"])]
    pub replace: bool,
    /// Write the rendered PNG to this file.
    #[arg(long, value_name = "FILE", conflicts_with_all = ["suggest_redactions", "redact_emails"])]
    pub out: Option<PathBuf>,
    /// Also copy the rendered image (annotations and redactions applied) to the clipboard.
    #[arg(long, conflicts_with_all = ["suggest_redactions", "redact_emails"])]
    pub copy: bool,
    /// Do not ask the daemon to send the rendered image back (`return_image: false`).
    #[arg(long, conflicts_with_all = ["out", "suggest_redactions", "redact_emails"])]
    pub no_image: bool,
    /// Read-only (`suggest_redactions`): find emails, API keys and other tokens (and phone
    /// numbers and IPv4 addresses) with on-device text recognition and print blur rectangles.
    /// Nothing is applied; pass the ones you accept to `annotate --ops` as `blur` ops.
    #[arg(long)]
    pub suggest_redactions: bool,
    /// Like `--suggest-redactions`, but email addresses only.
    #[arg(long, conflicts_with = "suggest_redactions")]
    pub redact_emails: bool,
}

#[derive(Debug, Args)]
pub struct RevisionArgs {
    /// Capture id.
    pub id: String,
    /// Write the resulting PNG to this file.
    #[arg(long, value_name = "FILE")]
    pub out: Option<PathBuf>,
}

#[derive(Debug, Args)]
pub struct ExportArgs {
    /// Capture id of a recording.
    pub id: String,
    /// Preset: `quick` (the default: screen + camera bubble + audio, H.264/AAC at the recording's
    /// size), a platform preset (`youtube-4k`, `youtube-1440p`, `youtube-1080p`,
    /// `vertical-1080`, `square-1080`, `source`; -14 LUFS audio) or `gif` / `webm`.
    #[arg(long, default_value = "quick", value_name = "PRESET")]
    pub preset: String,
    /// `gif` or `webm`: the same as `--preset gif` / `--preset webm`.
    #[arg(long, value_name = "FORMAT", value_parser = ["mp4", "gif", "webm"])]
    pub format: Option<String>,
    /// GIF and WebM width in px (GIF: 480, 640, 800 or 1024).
    #[arg(long, value_name = "PX")]
    pub width: Option<u32>,
    /// GIF and WebM frame rate (GIF: 10, 15 or 24).
    #[arg(long, value_name = "FPS")]
    pub fps: Option<f64>,
    /// GIF and WebM: fit the file under this many MB (10^6 bytes) by lowering the width, frame
    /// rate or bitrate.
    #[arg(long, value_name = "MB")]
    pub max_mb: Option<f64>,
    /// Normalise the integrated loudness to this level in LUFS (-40 to -5; -14 is the usual
    /// platform target), with a -1 dBTP true-peak limiter. Default: keep the recorded level.
    #[arg(long, value_name = "LUFS", allow_negative_numbers = true, value_parser = loudness_lufs)]
    pub loudness: Option<f64>,
    /// Write the captions as a file instead of a video: `srt`, `vtt` or `txt` (plain text, one
    /// caption line per row) in the capture's `exports/` folder (`export_captions`), with the
    /// recording's cuts and speed applied. Run `capturemann ai transcribe` first.
    #[arg(
        long,
        value_name = "FORMAT",
        value_parser = ["srt", "vtt", "txt"],
        conflicts_with_all = ["preset", "format", "width", "fps", "max_mb", "loudness", "wait", "timeout"]
    )]
    pub captions: Option<String>,
    /// With `--captions txt`: start every line with `[mm:ss]` (or `[hh:mm:ss]` for long
    /// recordings).
    #[arg(long, requires = "captions")]
    pub timestamps: bool,
    /// Burn the captions into the video (white text on a dark box, inside the platform's safe
    /// area). The default is the recording's `ai captions options --burn` setting. Needs a
    /// transcript: run `capturemann ai transcribe` first.
    #[arg(
        long = "burn-captions",
        conflicts_with_all = ["captions", "no_captions"]
    )]
    pub burn_captions: bool,
    /// Leave the captions out of the video even if the recording is set to burn them in.
    #[arg(long = "no-captions", conflicts_with = "captions")]
    pub no_captions: bool,
    /// Wait for the export to finish and print its file (exit 6 if it fails).
    #[arg(long)]
    pub wait: bool,
    /// With `--wait`: give up after this many seconds (exit 6; the export keeps running).
    /// Without it the command waits for as long as the job is queued, held or running.
    #[arg(long, value_name = "SECS", requires = "wait", value_parser = positive_secs)]
    pub timeout: Option<f64>,
}

fn loudness_lufs(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .parse()
        .map_err(|_| "must be a number of LUFS".to_owned())?;
    if v.is_finite() && (-40.0..=-5.0).contains(&v) {
        Ok(v)
    } else {
        Err("must be from -40 to -5 LUFS".to_owned())
    }
}

fn positive_secs(s: &str) -> Result<f64, String> {
    let v: f64 = s
        .parse()
        .map_err(|_| "must be a number of seconds".to_owned())?;
    if v.is_finite() && v > 0.0 {
        Ok(v)
    } else {
        Err("must be greater than 0".to_owned())
    }
}

#[derive(Debug, Args)]
pub struct UploadArgs {
    /// Capture id (or use `--file`).
    #[arg(required_unless_present = "file", conflicts_with = "file")]
    pub capture_id: Option<String>,
    /// Upload this file instead of a capture.
    #[arg(long)]
    pub file: Option<String>,
    /// Destination id (see `destinations`); the default destination when absent.
    #[arg(long = "to")]
    pub to: Option<String>,
    /// Folder inside the destination; replaces its folder pattern.
    #[arg(long)]
    pub folder: Option<String>,
    /// What to send of a capture: `export`, `original` or `both`.
    #[arg(long, value_parser = ["export", "original", "both"])]
    pub what: Option<String>,
}

#[derive(Debug, Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct PublishArgs {
    #[command(subcommand)]
    pub cmd: Option<PublishCmd>,
    #[command(flatten)]
    pub send: PublishSendArgs,
}

#[derive(Debug, Subcommand)]
pub enum PublishCmd {
    /// List the connected publish accounts with today's upload count (`list_publish_accounts`).
    Accounts,
}

#[derive(Debug, Args)]
pub struct PublishSendArgs {
    /// Capture id of the recording to publish.
    pub capture_id: Option<String>,
    /// Platform; only `youtube` is built.
    #[arg(long, default_value = "youtube")]
    pub platform: String,
    /// Account id from `publish accounts`; the default account when absent.
    #[arg(long)]
    pub account: Option<String>,
    /// Video title (required, at most 100 characters).
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub description: Option<String>,
    /// Tags, comma separated.
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    /// `private`, `unlisted` or `public`; the account's default (never public) when absent.
    #[arg(long, value_parser = ["private", "unlisted", "public"])]
    pub privacy: Option<String>,
    /// Is the video made for kids? Required by YouTube; there is no default.
    #[arg(long = "made-for-kids", value_parser = ["yes", "no"])]
    pub made_for_kids: Option<String>,
    /// Absolute path of a JPG or PNG thumbnail (up to 2 MB).
    #[arg(long)]
    pub thumbnail: Option<String>,
    /// Export with this preset first (for example `youtube-1080p`); the newest export otherwise.
    #[arg(long = "export-preset")]
    pub export_preset: Option<String>,
    /// Also upload the captions as an SRT track.
    #[arg(long)]
    pub captions: bool,
    /// YouTube category id (28, Science & Technology, when absent).
    #[arg(long)]
    pub category: Option<String>,
    #[arg(long)]
    pub playlist: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum LibraryCmd {
    /// List captures, newest first.
    List(LibraryListArgs),
    /// Full-text search (same as `list --query`).
    Search {
        text: String,
        #[command(flatten)]
        paging: LibraryPaging,
    },
    /// Rename a capture: sets its title and renames its folder (`rename_capture`).
    Rename {
        /// Capture id.
        id: String,
        /// The new title.
        title: String,
    },
    /// Move a capture to the Trash (`delete_capture`). The user must approve on screen. With
    /// `--also-remote` the dialog lists the remote copies that are deleted first.
    Rm {
        /// Capture id.
        id: String,
        /// Also delete the remote copies the capture records, PERMANENTLY: Google Drive files
        /// and folders, copies in local destination folders, YouTube videos. If one step fails
        /// the capture is kept and the error says what was and was not deleted.
        #[arg(long)]
        also_remote: bool,
    },
}

#[derive(Debug, Args)]
pub struct LibraryPaging {
    /// Only `screenshot` or `recording`.
    #[arg(long, value_parser = ["screenshot", "recording"])]
    pub kind: Option<String>,
    /// Only captures with or without upload or publish records.
    #[arg(long, value_parser = ["uploaded", "not_uploaded", "published", "not_published"])]
    pub status: Option<String>,
    /// Page size (1 to 200, default 50).
    #[arg(long)]
    pub limit: Option<usize>,
    /// `next_cursor` of the previous page.
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Debug, Args)]
pub struct LibraryListArgs {
    /// Search text (title match).
    #[arg(long)]
    pub query: Option<String>,
    #[command(flatten)]
    pub paging: LibraryPaging,
}

#[derive(Debug, Subcommand)]
pub enum SettingsCmd {
    /// Print settings (all, or the given dotted keys).
    Get { keys: Vec<String> },
    /// Set `KEY VALUE` pairs; a value is JSON when it parses (60, true), else a string.
    Set {
        #[arg(value_name = "KEY VALUE", required = true)]
        pairs: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum EditCmd {
    /// Keep only START..END (`trim`).
    Trim {
        /// Capture id.
        id: String,
        #[arg(long, allow_hyphen_values = true)]
        start: String,
        #[arg(long, allow_hyphen_values = true)]
        end: String,
    },
    /// Cut ranges like `10-15 40-42` (`cut_segments`); all are one undo step.
    Cut {
        id: String,
        #[arg(value_name = "START-END", required = true)]
        ranges: Vec<String>,
    },
    /// Bring cut ranges back (`restore_segments`).
    Restore {
        id: String,
        #[arg(value_name = "START-END", required = true)]
        ranges: Vec<String>,
    },
    /// Add a segment boundary (`split`).
    Split {
        id: String,
        #[arg(long, allow_hyphen_values = true)]
        at: String,
    },
    /// Play a range like `30-50` at `--x` times normal speed, 0.25 to 8 (`set_speed`).
    Speed {
        id: String,
        #[arg(value_name = "START-END")]
        range: String,
        /// The speed factor.
        #[arg(long = "x", value_name = "FACTOR")]
        factor: f64,
        /// Keep the audio above 2x (it is muted by default).
        #[arg(long)]
        keep_audio: bool,
    },
    /// Frame the screen on a background (`set_background`). Options left out keep their value.
    Background {
        id: String,
        #[arg(long, value_parser = ["none", "color", "gradient", "image"])]
        kind: String,
        /// `#rrggbb`, two colors `#rrggbb,#rrggbb` for a gradient, or an image path.
        #[arg(long)]
        value: Option<String>,
        /// Padding, percent of the frame (0 to 20).
        #[arg(long)]
        padding: Option<f64>,
        /// Screen corner radius in px (0 to 24).
        #[arg(long)]
        radius: Option<u32>,
        /// Draw a shadow under the screen.
        #[arg(long, value_parser = clap::value_parser!(bool))]
        shadow: Option<bool>,
    },
    /// Balance the mic and system audio (`set_audio_mix`). Options left out keep their value.
    Audio {
        id: String,
        /// Mic level in dB, -96 to 12.
        #[arg(long, allow_hyphen_values = true, value_name = "DB")]
        mic_gain: Option<f64>,
        /// System-audio level in dB, -96 to 12.
        #[arg(long, allow_hyphen_values = true, value_name = "DB")]
        system_gain: Option<f64>,
        /// Mute the mic.
        #[arg(long, value_parser = clap::value_parser!(bool))]
        mic_mute: Option<bool>,
        /// Mute the system audio.
        #[arg(long, value_parser = clap::value_parser!(bool))]
        system_mute: Option<bool>,
        /// Use the denoised mic file (true) or the raw one (false).
        #[arg(long, value_parser = clap::value_parser!(bool))]
        denoise: Option<bool>,
    },
    /// Set how the screen and the camera share the frame over a range like `0-15`
    /// (`set_camera_layout`). A later range overwrites the layouts it overlaps.
    #[command(alias = "layout")]
    Camera {
        id: String,
        #[arg(value_name = "START-END")]
        range: String,
        /// `bubble` (screen with the camera bubble), `screen` (no camera), `camera` (camera full
        /// frame) or `side_by_side` (screen 2/3, camera 1/3).
        #[arg(long, value_parser = ["bubble", "screen", "camera", "side_by_side"])]
        layout: String,
        /// Bubble shape (`bubble` only).
        #[arg(long, value_parser = ["circle", "rounded"])]
        shape: Option<String>,
        /// Bubble `x,y,w,h` as fractions of the frame, w from 0.05 to 1 (`bubble` only).
        #[arg(long, value_name = "X,Y,W,H")]
        rect: Option<String>,
        /// Ring width as a fraction of the bubble (0 to 0.5), or `none` (`bubble` only).
        #[arg(long)]
        ring: Option<String>,
        /// Mirror the camera (true) or not (false).
        #[arg(long, value_parser = clap::value_parser!(bool))]
        mirror: Option<bool>,
        /// Move smoothly from the layout before over 300 ms instead of switching at once.
        #[arg(long)]
        smooth: bool,
    },
    /// Add a zoom region (`set_zooms`): starts at `--at`, lasts `--length`, keeps `--focus` in the
    /// middle. A new region shortens the ones it overlaps.
    Zoom {
        id: String,
        /// When the zoom starts, in seconds or `t:<ticks>` (session time).
        #[arg(long, allow_hyphen_values = true, required_unless_present = "clear")]
        at: Option<String>,
        /// How long it lasts, in seconds (at least 0.8).
        #[arg(long, allow_hyphen_values = true, required_unless_present = "clear")]
        length: Option<String>,
        /// The point kept in the middle as `x,y`, fractions of the frame (default 0.5,0.5).
        #[arg(long, value_name = "X,Y")]
        focus: Option<String>,
        /// Zoom factor, 1.2 to 3 (default 1.8).
        #[arg(long)]
        scale: Option<f64>,
        /// Keep the cursor in view while zoomed.
        #[arg(long)]
        follow_cursor: bool,
        /// Replace every existing zoom with this one.
        #[arg(long)]
        replace: bool,
        /// Remove every zoom.
        #[arg(long, conflicts_with_all = ["at", "length", "focus", "scale", "follow_cursor", "replace"])]
        clear: bool,
    },
    /// Style the telemetry cursor (`set_cursor_style`). Options left out keep their value.
    Cursor {
        id: String,
        /// Size, 1 to 3 times.
        #[arg(long)]
        size: Option<f64>,
        #[arg(long, value_parser = ["off", "light", "strong"])]
        smoothing: Option<String>,
        #[arg(long, value_parser = ["none", "ring", "ripple"])]
        click_effect: Option<String>,
        /// Fade the cursor out after 2 s without movement.
        #[arg(long, value_parser = clap::value_parser!(bool))]
        hide_idle: Option<bool>,
    },
    /// Reframe for a vertical or square export (`set_reframe`).
    Reframe {
        id: String,
        /// The output aspect the setting is for.
        #[arg(long, value_parser = ["9:16", "1:1"])]
        aspect: String,
        #[arg(long, value_parser = ["fit", "fixed", "follow"])]
        mode: Option<String>,
        /// The window centre as `x,y`, fractions of the frame.
        #[arg(long, value_name = "X,Y")]
        focus: Option<String>,
        /// A time inside the segment the focus is for (default: the first segment).
        #[arg(long, allow_hyphen_values = true)]
        at: Option<String>,
    },
}

#[derive(Debug, Subcommand)]
pub enum DaemonCmd {
    /// Make sure a daemon is running (starts a headless one if needed).
    Start {
        /// Library folder for a daemon started by this command.
        #[arg(long, env = "CAPTUREMANN_LIBRARY", value_name = "DIR")]
        library: Option<PathBuf>,
    },
    /// Report whether a daemon is running (exit 5, `daemon_unavailable`, if not).
    Status,
}

#[derive(Debug, Subcommand)]
pub enum JobsCmd {
    /// Show one job: state, progress, result or error (`get_job`).
    Get {
        /// Job id.
        id: String,
    },
    /// List jobs, newest first (`list_jobs`).
    List {
        /// Only jobs in this state.
        #[arg(long, value_parser = ["queued", "running", "paused", "failed", "done", "cancelled"])]
        state: Option<String>,
        /// Only jobs of this type (`export`, `upload`, ...).
        #[arg(long = "type", value_name = "TYPE")]
        job_type: Option<String>,
        /// Page size (1 to 200, default 50).
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Cancel a job; cancelling a cancelled job does nothing (`cancel_job`).
    Cancel {
        /// Job id.
        id: String,
    },
    /// Retry a failed or cancelled job, or continue one that is waiting for a sign-in
    /// (`retry_job`).
    Retry {
        /// Job id.
        id: String,
    },
    /// Print the active jobs, then stream `job.*` events until interrupted.
    Watch {
        /// Event topics (default `job.*`).
        #[arg(long = "topic", value_name = "TOPIC")]
        topics: Vec<String>,
        /// Stop after this many events (for scripts); the initial snapshot does not count.
        #[arg(long, value_name = "N")]
        max_events: Option<usize>,
    },
}

#[derive(Debug, Subcommand)]
pub enum RecordCmd {
    /// Start a recording (`start_recording`). Asks for confirmation in the CaptureMann window
    /// and waits (up to 5 minutes) for the answer and for the countdown.
    Start(Box<RecordStartArgs>),
    /// Pause the running recording (`pause_recording`).
    Pause(RecordRefArgs),
    /// Resume a paused recording (`resume_recording`).
    Resume(RecordRefArgs),
    /// Stop and save the recording (`stop_recording`). Also cancels a countdown and a running
    /// 60 fps self-test.
    Stop(RecordRefArgs),
    /// Show the state of the recording: elapsed time, tracks, mic level, disk (`get_recording_status`).
    Status(RecordRefArgs),
    /// Test whether this machine records 60 fps without late frames (`run_fps_selftest`): records
    /// about 10 s of the screen, measures late and dropped frames, deletes the video and keeps only
    /// the result. 60 fps is accepted by `record start --fps 60` only after a pass. Asks for
    /// confirmation in the CaptureMann window when run by an agent.
    Selftest(RecordSelftestArgs),
    /// Stream `recording.*` events until interrupted.
    Watch {
        /// Event topics (default `recording.*`).
        #[arg(long = "topic", value_name = "TOPIC")]
        topics: Vec<String>,
        /// Stop after this many events (for scripts).
        #[arg(long, value_name = "N")]
        max_events: Option<usize>,
    },
}

#[derive(Debug, Args)]
pub struct RecordSelftestArgs {
    /// Display id to test (see `targets`; default: the primary display).
    #[arg(long, value_name = "ID")]
    pub display: Option<u32>,
    /// Output size to test (default: the `recording.size` setting, else source).
    #[arg(long, value_parser = ["source", "2160p", "1440p", "1080p", "720p"])]
    pub size: Option<String>,
    /// Video codec to test (default: the `recording.codec` setting, else auto).
    #[arg(long, value_parser = ["auto", "hevc", "h264"])]
    pub codec: Option<String>,
}

#[derive(Debug, Args)]
pub struct RecordRefArgs {
    /// Recording id (default: the running recording).
    #[arg(long = "id", value_name = "RECORDING_ID")]
    pub recording_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct RecordStartArgs {
    /// Display id to record (see `targets`; default: the primary display).
    #[arg(long, value_name = "ID", conflicts_with = "window")]
    pub display: Option<u32>,
    /// Record one window: its id, or a case-insensitive substring of its title or app name.
    #[arg(long, value_name = "ID|TITLE", conflicts_with_all = ["display", "region"])]
    pub window: Option<String>,
    /// Record a region `x,y,w,h` in physical px, relative to the display.
    #[arg(long, value_name = "X,Y,W,H", allow_hyphen_values = true)]
    pub region: Option<String>,
    /// Output size.
    #[arg(long, value_parser = ["source", "2160p", "1440p", "1080p", "720p"])]
    pub size: Option<String>,
    /// Frame rate.
    #[arg(long, value_parser = ["30", "60"])]
    pub fps: Option<String>,
    /// Video codec (default: HEVC above 1920 px, else H.264).
    #[arg(long, value_parser = ["auto", "hevc", "h264"])]
    pub codec: Option<String>,
    /// Bitrate preset.
    #[arg(long, value_parser = ["high", "balanced", "small"])]
    pub quality: Option<String>,
    /// Record the webcam: `on` (default camera), `off`, or a camera id (see `devices`).
    #[arg(long, value_name = "on|off|ID", num_args = 0..=1, default_missing_value = "on")]
    pub camera: Option<String>,
    /// Camera resolution.
    #[arg(long, value_parser = ["720p", "1080p"])]
    pub camera_resolution: Option<String>,
    /// Webcam bubble shape.
    #[arg(long, value_parser = ["circle", "rounded"])]
    pub camera_shape: Option<String>,
    /// Webcam bubble size (small 160, medium 240, large 320 px).
    #[arg(long, value_parser = ["s", "m", "l"])]
    pub camera_size: Option<String>,
    /// Webcam bubble position (default `bottom-left`): `bottom-right`, `top-left`, ... or `X,Y` as fractions 0..1.
    #[arg(long, value_name = "POSITION", allow_hyphen_values = true)]
    pub camera_pos: Option<String>,
    /// Store the camera display mirror flag off (the recorded pixels are never flipped).
    #[arg(long)]
    pub no_camera_mirror: bool,
    /// Record the microphone: `default`, `off`, or a microphone id (see `devices`).
    #[arg(long, value_name = "default|off|ID", num_args = 0..=1, default_missing_value = "default")]
    pub mic: Option<String>,
    /// Do not write the noise-suppressed microphone track.
    #[arg(long)]
    pub no_denoise: bool,
    /// Also record system audio.
    #[arg(long)]
    pub system_audio: bool,
    /// Do not record cursor telemetry (the pointer stays in the video).
    #[arg(long)]
    pub no_cursor: bool,
    /// Record the cursor separately: it is left out of the video and drawn from the telemetry, so
    /// the editor can restyle it (`edit cursor`). Default: the `recording.cursor_separate` setting.
    #[arg(long, conflicts_with = "no_cursor")]
    pub cursor_separate: bool,
    /// Countdown before recording, 0 to 10 seconds (default: the `recording.countdown_s` setting, 3).
    #[arg(long, value_name = "SECONDS")]
    pub countdown: Option<u32>,
    /// Stop automatically after this many seconds of recorded time.
    #[arg(long, value_name = "SECONDS")]
    pub max_duration: Option<f64>,
    /// Library title (default: `Recording yyyy-MM-dd HH.mm`).
    #[arg(long)]
    pub title: Option<String>,
    /// Keep this app (bundle id) out of the recording, in addition to the defaults; repeatable.
    #[arg(long = "exclude-app", value_name = "BUNDLE_ID")]
    pub exclude_apps: Vec<String>,
    /// Keep this window id out of the recording; repeatable.
    #[arg(long = "exclude-window", value_name = "ID")]
    pub exclude_windows: Vec<u32>,
}
