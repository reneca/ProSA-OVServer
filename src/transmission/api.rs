use std::fmt;

use base64::{Engine as _, engine::general_purpose::STANDARD as base64};
use chrono::{DateTime, TimeDelta, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeMap as _};
use serde_repr::{Deserialize_repr, Serialize_repr};
use serde_with::skip_serializing_none;

fn serialize_method_ids<S>(
    serializer: S,
    method_name: &str,
    ids: &Option<TorrentId>,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut map = serializer.serialize_map(if ids.is_some() { Some(2) } else { Some(1) })?;
    map.serialize_entry("method", method_name)?;
    if let Some(ids) = ids.as_ref().map(ArgumentsId::from) {
        map.serialize_entry("arguments", &ids)?;
    }
    map.end()
}

fn deserialize_timestamp<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<i64>::deserialize(deserializer)?.and_then(|ts| {
        if ts > 0 {
            DateTime::from_timestamp_secs(ts)
        } else {
            None
        }
    }))
}

fn deserialize_time<'de, D>(deserializer: D) -> Result<Option<TimeDelta>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<i64>::deserialize(deserializer)?
        .and_then(|ts| if ts > 0 { TimeDelta::new(ts, 0) } else { None }))
}

fn deserialize_bitfield<'de, D>(deserializer: D) -> Result<Option<Vec<u8>>, D::Error>
where
    D: Deserializer<'de>,
{
    let encoded: &str = Deserialize::deserialize(deserializer)?;
    Ok(base64.decode(encoded).ok())
}

#[derive(Debug, Clone)]
pub enum Method {
    /// start torrent
    TorrentStart(Option<TorrentId>),
    /// start torrent disregarding queue position
    TorrentStartNow(Option<TorrentId>),
    /// stop torrent
    TorrentStop(Option<TorrentId>),
    /// verify torrent
    TorrentVerify(Option<TorrentId>),
    /// re-announce to trackers now
    TorrentReannounce(Option<TorrentId>),
    /// torrent mutator
    TorrentSet(Box<TorrentSetParams>),
    /// torrent getter
    TorrentGet(Vec<TorrentField>, Option<Vec<TorrentId>>),
    /// remove torrent, delete local data if bool is true
    TorrentRemove(Vec<TorrentId>, bool),
    /// Session statistics for all torrents
    SessionStats,
}

impl Serialize for Method {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Method::TorrentStart(ids) => serialize_method_ids(serializer, "torrent-start", ids),
            Method::TorrentStartNow(ids) => {
                serialize_method_ids(serializer, "torrent-start-now", ids)
            }
            Method::TorrentStop(ids) => serialize_method_ids(serializer, "torrent-stop", ids),
            Method::TorrentVerify(ids) => serialize_method_ids(serializer, "torrent-verify", ids),
            Method::TorrentReannounce(ids) => {
                serialize_method_ids(serializer, "torrent-reannounce", ids)
            }
            Method::TorrentSet(params) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("method", "torrent-set")?;
                map.serialize_entry("arguments", &params)?;
                map.end()
            }
            Method::TorrentGet(fields, ids) => {
                let mut map =
                    serializer.serialize_map(if ids.is_some() { Some(2) } else { Some(1) })?;
                map.serialize_entry("method", "torrent-get")?;
                map.serialize_entry("arguments", &ArgumentsTorrentGet { fields, ids })?;
                map.end()
            }
            Method::TorrentRemove(ids, delete_local_data) => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("method", "torrent-remove")?;
                map.serialize_entry(
                    "arguments",
                    &ArgumentsTorrentRemove {
                        ids,
                        delete_local_data: *delete_local_data,
                    },
                )?;
                map.end()
            }
            Method::SessionStats => {
                let mut map = serializer.serialize_map(Some(1))?;
                map.serialize_entry("method", "session-stats")?;
                map.end()
            }
        }
    }
}

/// Torrent id number or SHA1 hash strings
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(untagged)]
pub enum Id {
    Id(i64),
    Hash(String),
}

impl From<i64> for Id {
    fn from(id: i64) -> Self {
        Id::Id(id)
    }
}

impl From<String> for Id {
    fn from(hash: String) -> Self {
        Id::Hash(hash)
    }
}

#[derive(Serialize)]
struct ArgumentsId<'a> {
    ids: &'a TorrentId,
}

impl<'a> From<&'a TorrentId> for ArgumentsId<'a> {
    fn from(ids: &'a TorrentId) -> Self {
        ArgumentsId { ids }
    }
}

#[derive(Serialize)]
struct ArgumentsTorrentGet<'a> {
    fields: &'a Vec<TorrentField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ids: &'a Option<Vec<TorrentId>>,
}

#[derive(Serialize)]
struct ArgumentsTorrentRemove<'a> {
    ids: &'a Vec<TorrentId>,
    delete_local_data: bool,
}

/// Torrent ids should be one of the following:
/// - an integer referring to a torrent id
/// - a list of torrent id numbers, SHA1 hash strings, or both
/// - a string, recently-active, for recently-active torrents
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TorrentId {
    #[serde(rename = "recently-active")]
    RecentlyActive,
    #[serde(untagged)]
    Id(Id),
    #[serde(untagged)]
    List(Vec<Id>),
}

impl From<i64> for TorrentId {
    fn from(id: i64) -> Self {
        TorrentId::Id(id.into())
    }
}

impl From<String> for TorrentId {
    fn from(hash: String) -> Self {
        TorrentId::Id(hash.into())
    }
}

impl From<Id> for TorrentId {
    fn from(id: Id) -> Self {
        TorrentId::Id(id)
    }
}

impl From<Vec<Id>> for TorrentId {
    fn from(ids: Vec<Id>) -> Self {
        TorrentId::List(ids)
    }
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i8)]
pub enum Priority {
    Low = -1,
    Normal = 0,
    High = 1,
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatErrType {
    /// everything's fine
    Ok,
    /// when we announced to the tracker, we got a warning in the response
    TrackerWarning,
    /// when we announced to the tracker, we got an error in the response
    TrackerError,
    /// local trouble, such as disk full or permissions error
    LocalError,
    /// Unknown error
    #[default]
    Unknown,
}

impl<'de> Deserialize<'de> for StatErrType {
    fn deserialize<D>(deserializer: D) -> Result<StatErrType, D::Error>
    where
        D: Deserializer<'de>,
    {
        match i8::deserialize(deserializer)? {
            0 => Ok(StatErrType::Ok),
            1 => Ok(StatErrType::TrackerWarning),
            2 => Ok(StatErrType::TrackerError),
            3 => Ok(StatErrType::LocalError),
            _ => Ok(StatErrType::Unknown),
        }
    }
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum IdleLimit {
    /// follow the global settings
    Global = 0,
    /// override the global settings, seeding until a certain idle time
    Single = 1,
    /// override the global settings, seeding regardless of activity
    Unlimited = 2,
}

#[derive(Serialize_repr, Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RatioLimit {
    /// follow the global settings
    Global = 0,
    /// override the global settings, seeding until a certain ratio
    Single = 1,
    /// override the global settings, seeding regardless of activity
    Unlimited = 2,
}

#[derive(Deserialize, Debug, Clone)]
pub struct File {
    #[serde(rename = "bytesCompleted")]
    pub bytes_completed: u64,
    pub length: u64,
    pub name: String,
    pub begin_piece: u32,
    pub end_piece: u32,
}

#[derive(Deserialize, Debug, Clone)]
pub struct FileStat {
    #[serde(rename = "bytesCompleted")]
    pub bytes_completed: f64,
    pub wanted: bool,
    pub priority: Priority,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Peer {
    pub address: String,
    #[serde(rename = "bytes_to_client")]
    pub bytes_to_client: u64,
    #[serde(rename = "bytes_to_peer")]
    pub bytes_to_peer: u64,
    pub client_name: String,
    pub client_is_choked: bool,
    pub client_is_interested: bool,
    pub flag_str: String,
    pub is_downloading_from: bool,
    pub is_encrypted: bool,
    pub is_incoming: bool,
    pub is_uploading_to: bool,
    pub is_utp: bool,
    pub peer_is_choked: bool,
    pub peer_is_interested: bool,
    #[serde(rename = "peer_id")]
    pub peer_id: String,
    pub port: u16,
    pub progress: f64,
    pub rate_to_client: u32,
    pub rate_to_peer: u32,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PeerFrom {
    pub from_cache: u64,
    pub from_dht: u64,
    pub from_incoming: u64,
    pub from_lpd: u64,
    pub from_ltpep: u64,
    pub from_pex: u64,
    pub from_tracker: u64,
}

#[derive(Deserialize_repr, Debug, Clone, Copy, Hash, PartialEq, Eq)]
#[repr(u8)]
pub enum Status {
    /// Torrent is stopped
    Stopped = 0,
    /// Queued to check files
    CheckWait = 1,
    /// Checking files
    Check = 2,
    /// Queued to download
    DownloadWait = 3,
    /// Downloading
    Download = 4,
    /// Queued to seed
    SeedWait = 5,
    /// Seeding
    Seed = 6,
}

impl Status {
    pub fn iterator() -> impl Iterator<Item = Status> {
        [
            Status::Stopped,
            Status::CheckWait,
            Status::Check,
            Status::DownloadWait,
            Status::Download,
            Status::SeedWait,
            Status::Seed,
        ]
        .iter()
        .copied()
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Status::Stopped => write!(f, "stopped"),
            Status::CheckWait => write!(f, "check_wait"),
            Status::Check => write!(f, "check"),
            Status::DownloadWait => write!(f, "dwonload_wait"),
            Status::Download => write!(f, "download"),
            Status::SeedWait => write!(f, "seed_wait"),
            Status::Seed => write!(f, "seed"),
        }
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct Tracker {
    /// full announce URL
    pub announce: String,
    /// unique transmission-generated ID for use in libtransmission API
    pub id: u32,
    /// full scrape URL
    pub scrape: String,
    /// The tracker site's name. Uses the first label before the public suffix
    /// (https://publicsuffix.org/) in the announce URL's host.
    /// e.g. "https://www.example.co.uk/announce/"'s sitename is "example"
    /// RFC 1034 says labels must be less than 64 chars
    pub sitename: String,
    /// which tier this tracker is in
    pub tier: i64,
}

#[derive(Deserialize_repr, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrackerState {
    /// we won't (announce,scrape) this torrent to this tracker because
    /// the torrent is stopped, or because of an error, or whatever
    Inactive = 0,
    /// we will (announce,scrape) this torrent to this tracker, and are
    /// waiting for enough time to pass to satisfy the tracker's interval
    Waiting = 1,
    /// it's time to (announce,scrape) this torrent, and we're waiting on a
    /// free slot to open up in the announce manager
    Queued = 2,
    /// we're (announcing,scraping) this torrent right now
    Active = 3,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TrackerStat {
    /// full announce URL
    pub announce: String,
    /// whether we're announcing, waiting to announce, etc
    pub announce_state: TrackerState,
    /// number of times this torrent's been downloaded, or -1 if unknown
    pub download_count: i64,
    /// number of downloaders (BEP-21) the tracker knows of, or -1 if unknown
    #[serde(rename = "downloader_count")]
    pub downloader_count: i64,
    /// true if we've announced to this tracker during this session
    #[serde(default)]
    pub has_announced: bool,
    /// true if we've scraped this tracker during this session
    #[serde(default)]
    pub has_scraped: bool,
    /// uniquely-identifying tracker name
    pub host: String,
    /// unique transmission-generated ID for use in libtransmission API
    pub id: u32,
    /// only one tracker per tier is used; the others are kept as backups
    #[serde(default)]
    pub is_backup: bool,
    /// if hasAnnounced, the number of peers the tracker gave us
    pub last_announce_peer_count: Option<i64>,
    /// if hasAnnounced, the human-readable result of latest announce
    pub last_announce_result: Option<String>,
    /// if hasAnnounced, when the latest announce request was sent
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub last_announce_start_time: Option<DateTime<Utc>>,
    /// if hasAnnounced, whether or not the latest announce succeeded
    #[serde(default)]
    pub last_announce_succeeded: bool,
    /// if hasAnnounced, when the latest announce reply was received
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub last_announce_time: Option<DateTime<Utc>>,
    /// true if the latest announce request timed out
    #[serde(default)]
    pub last_announce_timed_out: bool,
    /// if hasScraped, the human-readable result of the latest scrape
    pub last_scrape_result: Option<String>,
    /// if hasScraped, when the latest scrape request was sent
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub last_scrape_start_time: Option<DateTime<Utc>>,
    /// if hasScraped, whether or not the latest scrape succeeded
    #[serde(default)]
    pub last_scrape_succeeded: bool,
    /// if hasScraped, when the latest scrape reply was received
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub last_scrape_time: Option<DateTime<Utc>>,
    /// true if the latest scrape request timed out
    #[serde(default)]
    pub last_scrape_timed_out: bool,
    /// number of leechers the tracker knows of, or -1 if unknown
    pub leecher_count: i64,
    /// if announceState == TR_TRACKER_WAITING, time of next announce
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub next_announce_time: Option<DateTime<Utc>>,
    /// if scrapeState == TR_TRACKER_WAITING, time of next scrape
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub next_scrape_time: Option<DateTime<Utc>>,
    /// full scrape URL
    pub scrape: String,
    /// whether we're scraping, waiting to scrape, etc
    pub scrape_state: TrackerState,
    /// number of seeders the tracker knows of, or -1 if unknown
    pub seeder_count: i64,
    /// The tracker site's name. Uses the first label before the public suffix
    /// (https://publicsuffix.org/) in the announce URL's host.
    /// e.g. "https://www.example.co.uk/announce/"'s sitename is "example"
    /// RFC 1034 says labels must be less than 64 chars
    pub sitename: String,
    /// which tier this tracker is in
    pub tier: i64,
}

#[skip_serializing_none]
#[derive(Serialize, Default, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TorrentSetParams {
    /// this torrent's bandwidth priority
    pub bandwidth_priority: Option<Priority>,
    /// maximum download speed (kB/s)
    pub download_limit: Option<u64>,
    /// true if download_limit is honored
    pub download_limited: Option<bool>,
    /// indices of file(s) to not download
    #[serde(rename = "files-unwanted")]
    pub files_unwanted: Option<Vec<usize>>,
    /// indices of file(s) to download
    #[serde(rename = "files-wanted")]
    pub files_wanted: Option<Vec<usize>>,
    /// The name of this torrent's bandwidth group
    pub group: Option<String>,
    /// true if session upload limits are honored
    pub honors_session_limits: Option<bool>,
    /// torrent list
    pub ids: Option<Vec<TorrentId>>,
    /// array of string labels
    pub labels: Option<Vec<String>>,
    /// new location of the torrent's content
    pub location: Option<String>,
    /// maximum number of peers
    #[serde(rename = "peer-limit")]
    pub peer_limit: Option<u64>,
    /// indices of high-priority file(s)
    #[serde(rename = "priority-high")]
    pub priority_high: Option<Vec<u32>>,
    /// indices of low-priority file(s)
    #[serde(rename = "priority-low")]
    pub priority_low: Option<Vec<u32>>,
    /// indices of normal-priority file(s)
    #[serde(rename = "priority-normal")]
    pub priority_normal: Option<Vec<u32>>,
    /// position of this torrent in its queue [0...n)
    pub queue_position: Option<u32>,
    /// torrent-level number of minutes of seeding inactivity
    pub seed_idle_limit: Option<u64>,
    /// which seeding inactivity to use
    pub seed_idle_mode: Option<IdleLimit>,
    /// torrent-level seeding ratio
    pub seed_ratio_limit: Option<f64>,
    /// which ratio to use
    pub seed_ratio_mode: Option<RatioLimit>,
    /// download torrent pieces sequentially
    pub sequential_download: Option<bool>,
    /// download from a specific piece when sequential download is enabled
    pub sequential_download_from_piece: Option<u32>,
    /// DEPRECATED use tracker_list instead
    pub tracker_add: Option<Vec<String>>,
    /// string of announce URLs, one per line, and a blank line between tiers.
    pub tracker_list: Option<String>,
    /// DEPRECATED use tracker_list instead
    pub tracker_remove: Option<Vec<String>>,
    /// DEPRECATED use tracker_list instead
    pub tracker_replace: Option<Vec<String>>,
    /// maximum upload speed (kB/s)
    pub upload_limit: Option<u64>,
    /// true if upload_limit is honored
    pub upload_limited: Option<bool>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum TorrentField {
    /// The last time we uploaded or downloaded piece data on this torrent
    ActivityDate,
    /// When the torrent was first added
    AddedDate,
    Availability,
    BandwidthPriority,
    BytesCompleted,
    Comment,
    CorruptEver,
    Creator,
    DateCreated,
    DesiredAvailable,
    DoneDate,
    DownloadDir,
    DownloadedEver,
    DownloadLimit,
    DownloadLimited,
    EditDate,
    Error,
    ErrorString,
    Eta,
    EtaIdle,
    #[serde(rename = "file-count")]
    FileCount,
    Files,
    FileStats,
    Group,
    HashString,
    HaveUnchecked,
    HaveValid,
    HonorsSessionLimits,
    Id,
    IsFinished,
    IsPrivate,
    IsStalled,
    Labels,
    LeftUntilDone,
    MagnetLink,
    MaxConnectedPeers,
    MetadataPercentComplete,
    Name,
    #[serde(rename = "peer-limit")]
    PeerLimit,
    Peers,
    PeersConnected,
    PeersFrom,
    PeersGettingFromUs,
    PeersSendingToUs,
    PercentComplete,
    PercentDone,
    Pieces,
    PieceCount,
    PieceSize,
    Priorities,
    PrimaryMimeType,
    QueuePosition,
    RateDownload,
    RateUpload,
    RecheckProgress,
    SecondsDownloading,
    SecondsSeeding,
    SeedIdleLimit,
    SeedIdleMode,
    SeedRatioLimit,
    SeedRatioMode,
    SequentialDownload,
    SequentialDownloadFromPiece,
    SizeWhenDone,
    StartDate,
    Status,
    Trackers,
    TrackerList,
    TrackerStats,
    TotalSize,
    TorrentFile,
    UploadedEver,
    UploadLimit,
    UploadLimited,
    UploadRatio,
    Wanted,
    Webseeds,
    WebseedsSendingToUs,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TorrentsArguments {
    /// The last time we uploaded or downloaded piece data on this torrent
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub activity_date: Option<DateTime<Utc>>,
    /// When the torrent was first added
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub added_date: Option<DateTime<Utc>>,
    /// An array of pieceCount numbers representing the number of connected peers that have each piece, or -1 if we already have the piece ourselves
    pub availability: Option<Vec<u32>>,
    /// Bandwidth priority of the torrent
    pub bandwidth_priority: Option<Priority>,
    /// An array of `tr_info.filecount` numbers. Each is the completed bytes for the corresponding file
    pub bytes_completed: Option<Vec<i64>>,
    /// Comment of the torrent
    pub comment: Option<String>,
    /// Byte count of all the corrupt data you've ever downloaded for this torrent.
    /// If you're on a poisoned torrent, this number can grow very large
    pub corrupt_ever: Option<u64>,
    /// Creator name of the torrent
    pub creator: Option<String>,
    /// Creation date of the torrent
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub date_created: Option<DateTime<Utc>>,
    /// Byte count of all the piece data we want and don't have yet, but that a connected peer does have. [0...leftUntilDone]
    pub desired_available: Option<u64>,
    /// When the torrent finished downloading
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub done_date: Option<DateTime<Utc>>,
    /// Torrent download folder
    pub download_dir: Option<String>,
    /// Byte count of all the non-corrupt data you've ever downloaded for this torrent.
    /// If you deleted the files and downloaded a second time, this will be `2*totalSize`..
    pub downloaded_ever: Option<u64>,
    /// Limit of the download in kBps
    pub download_limit: Option<u64>,
    /// Indicate if the download is limited
    #[serde(default)]
    pub download_limited: bool,
    /// The last time during this session that a rarely-changing field changed
    /// e.g. any `tr_torrent_metainfo` field (trackers, filenames, name) or download directory.
    /// RPC clients can monitor this to know when to reload fields that rarely change
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub edit_date: Option<DateTime<Utc>>,
    /// Defines what kind of text is in errorString
    pub error: Option<StatErrType>,
    /// A warning or error message regarding the torrent
    pub error_string: Option<String>,
    /// If downloading, estimated number of seconds left until the torrent is done.
    /// If seeding, estimated number of seconds left until seed ratio is reached
    #[serde(deserialize_with = "deserialize_time", default)]
    pub eta: Option<TimeDelta>,
    /// If seeding, number of seconds left until the idle time limit is reached
    #[serde(deserialize_with = "deserialize_time", default)]
    pub eta_idle: Option<TimeDelta>,
    /// Number of file in the torrent
    #[serde(rename = "file-count")]
    pub file_count: Option<u64>,
    /// Files are returned in the order they are laid out in the torrent.
    /// References to "file indices" throughout this specification should be interpreted as the position of the file within this ordering, with the first file bearing index 0
    pub files: Option<Vec<File>>,
    /// File's non-constant properties.
    /// An array of tr_info.filecount objects, in the same order as `files`
    pub file_stats: Option<Vec<FileStat>>,
    /// Group name
    pub group: Option<String>,
    /// Hash value of the torrent
    pub hash_string: Option<String>,
    /// Byte count of all the partial piece data we have for this torrent.
    /// As pieces become complete, this value may decrease as portions of it are moved to `corrupt` or `haveValid`
    pub have_unchecked: Option<u64>,
    /// Byte count of all the checksum-verified data we have for this torrent
    pub have_valid: Option<u64>,
    /// Indicate if session upload limits are honored
    #[serde(default)]
    pub honors_session_limits: bool,
    /// Torrent ID
    pub id: Option<TorrentId>,
    #[serde(default)]
    pub is_finished: bool,
    #[serde(default)]
    pub is_private: bool,
    #[serde(default)]
    pub is_stalled: bool,
    pub labels: Option<Vec<String>>,
    /// Byte count of how much data is left to be downloaded until we've got all the pieces that we want. [0...tr_stat.sizeWhenDone]
    pub left_until_done: Option<u64>,
    pub magnet_link: Option<String>,
    /// DEPRECATED don't use it, it never worked
    pub manual_announce_time: Option<u64>,
    pub max_connected_peers: Option<u64>,
    pub metadata_percent_complete: Option<f64>,
    pub name: Option<String>,
    #[serde(rename = "peer-limit")]
    pub peer_limit: Option<u64>,
    pub peers: Option<Vec<Peer>>,
    /// Number of peers that we're connected to
    pub peers_connected: Option<u16>,
    pub peers_from: Option<PeerFrom>,
    /// Number of peers that we're sending data to
    pub peers_getting_from_us: Option<u16>,
    /// Number of peers that are sending data to us
    pub peers_sending_to_us: Option<u16>,
    /// How much has been downloaded of the entire torrent. Range is [0..1]
    pub percent_complete: Option<f64>,
    /// How much has been downloaded of the files the user wants.
    /// This differs from percentComplete if the user wants only some of the torrent's files. Range is [0..1]
    pub percent_done: Option<f64>,
    #[serde(deserialize_with = "deserialize_bitfield", default)]
    pub pieces: Option<Vec<u8>>,
    pub piece_count: Option<u32>,
    pub piece_size: Option<u64>,
    pub priorities: Option<Vec<Priority>>,
    #[serde(rename = "primary-mime-type")]
    pub primary_mime_type: Option<String>,
    /// This torrent's queue position.
    /// All torrents have a queue position, even if it's not queued
    pub queue_position: Option<usize>,
    /// Download rate in B/s
    pub rate_download: Option<i64>,
    /// Upload rate in B/s
    pub rate_upload: Option<i64>,
    /// When `tr_stat.activity` is `TR_STATUS_CHECK` or `TR_STATUS_CHECK_WAIT`, this is the percentage of how much of the files has been verified.
    /// When it gets to 1, the verify process is done. Range is [0..1]
    pub recheck_progress: Option<f32>,
    /// Cumulative seconds the torrent's ever spent downloading
    #[serde(deserialize_with = "deserialize_time", default)]
    pub seconds_downloading: Option<TimeDelta>,
    /// Cumulative seconds the torrent's ever spent seeding
    #[serde(deserialize_with = "deserialize_time", default)]
    pub seconds_seeding: Option<TimeDelta>,
    pub seed_idle_limit: Option<i64>,
    pub seed_idle_mode: Option<IdleLimit>,
    pub seed_ratio_limit: Option<f64>,
    pub seed_ratio_mode: Option<RatioLimit>,
    /// Enable sequential download
    #[serde(rename = "sequential_download", default)]
    pub sequential_download: bool,
    /// Enable sequential download from piece
    #[serde(rename = "sequential_download_from_piece")]
    pub sequential_download_from_piece: Option<i64>,
    /// Byte count of all the piece data we'll have downloaded when we're done, whether or not we have it yet.
    /// If we only want some of the files, this may be less than `tr_torrent_view.total_size`. [0...tr_torrent_view.total_size]
    pub size_when_done: Option<u64>,
    #[serde(deserialize_with = "deserialize_timestamp", default)]
    pub start_date: Option<DateTime<Utc>>,
    pub status: Option<Status>,
    pub trackers: Option<Vec<Tracker>>,
    /// string of announce URLs, one per line, with a blank line between tiers
    pub tracker_list: Option<String>,
    pub tracker_stats: Option<Vec<TrackerStat>>,
    pub total_size: Option<u64>,
    pub torrent_file: Option<String>,
    /// Byte count of all data you've ever uploaded for this torrent
    pub uploaded_ever: Option<u64>,
    pub upload_limit: Option<i64>,
    #[serde(default)]
    pub upload_limited: bool,
    pub upload_ratio: Option<f64>,
    /// An array of tr_torrentFileCount() 0/1, 1 (true) if the corresponding file is to be downloaded. (Source: tr_file_view)
    pub wanted: Option<Vec<bool>>,
    pub webseeds: Option<Vec<String>>,
    /// Number of webseeds that are sending data to us
    pub webseeds_sending_to_us: Option<u16>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TorrentAdded {
    /// Hash value of the torrent
    pub hash_string: Option<String>,
    /// Torrent ID
    pub id: Option<TorrentId>,
    pub name: Option<String>,
}

#[derive(Deserialize, Debug, Default, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    /// Uploaded bytes in this session
    pub uploaded_bytes: u64,
    /// Downloaded bytes in this session
    pub downloaded_bytes: u64,
    /// Number of files added in this session
    pub files_added: u32,
    /// Cumulative seconds the torrent's ever spent downloading
    #[serde(deserialize_with = "deserialize_time", default)]
    pub seconds_active: Option<TimeDelta>,
    /// Number of sessions
    pub session_count: u64,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ResponseArguments {
    pub version: Option<String>,
    #[serde(default)]
    pub torrents: Vec<TorrentsArguments>,
    #[serde(rename = "torrent-added")]
    pub torrent_added: Option<TorrentAdded>,

    pub active_torrent_count: Option<u32>,
    pub download_speed: Option<u64>,
    pub paused_torrent_count: Option<u32>,
    pub torrent_count: Option<u32>,
    pub upload_speed: Option<u64>,
    pub cumulative_stats: Option<SessionStats>,
    pub current_stats: Option<SessionStats>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Response {
    pub arguments: ResponseArguments,
    pub result: String,
    pub tag: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn torrent_id() {
        let id = TorrentId::from(7);
        assert_eq!(Some("7"), serde_json::to_string(&id).ok().as_deref());

        let id_hash = TorrentId::from("00F453355B28E4158D4E5E6A2D3EDA96B3450406".to_string());
        assert_eq!(
            Some("\"00F453355B28E4158D4E5E6A2D3EDA96B3450406\""),
            serde_json::to_string(&id_hash).ok().as_deref()
        );

        let ids = TorrentId::from(vec![
            Id::from(7),
            Id::from("00F453355B28E4158D4E5E6A2D3EDA96B3450406".to_string()),
        ]);
        assert_eq!(
            Some("[7,\"00F453355B28E4158D4E5E6A2D3EDA96B3450406\"]"),
            serde_json::to_string(&ids).ok().as_deref()
        );

        assert_eq!(
            Some("\"recently-active\""),
            serde_json::to_string(&TorrentId::RecentlyActive)
                .ok()
                .as_deref()
        );
    }

    #[test]
    fn torrent() {
        let torrent_start = Method::TorrentStart(None);
        assert_eq!(
            Some("{\"method\":\"torrent-start\"}"),
            serde_json::to_string(&torrent_start).ok().as_deref()
        );

        let torrent_stop = Method::TorrentStop(Some(1.into()));
        assert_eq!(
            Some("{\"method\":\"torrent-stop\",\"arguments\":{\"ids\":1}}"),
            serde_json::to_string(&torrent_stop).ok().as_deref()
        );
    }

    #[test]
    fn torrent_test() {
        let request = Method::TorrentGet(
            vec![
                TorrentField::Id,
                TorrentField::Name,
                TorrentField::TotalSize,
            ],
            Some(vec![7.into(), 10.into()]),
        );
        assert_eq!(
            Some(
                "{\"method\":\"torrent_get\",\"arguments\":{\"fields\":[\"id\",\"name\",\"totalSize\"],\"ids\":[7,10]}}"
            ),
            serde_json::to_string(&request).ok().as_deref()
        );

        let response = r#"
            {
                "arguments": {
                    "torrents": [
                        {
                            "id": 10,
                            "name": "Fedora x86_64 DVD",
                            "totalSize": 34983493932
                        },
                        {
                            "id": 7,
                            "name": "Ubuntu x86_64 DVD",
                            "totalSize": 9923890123
                        }
                    ]
                },
                "result": "success",
                "tag": 39693
            }"#;
        let v: Response = serde_json::from_str(response).unwrap();
        assert_eq!(Some(39693), v.tag);
        assert_eq!("success", v.result);
        assert_eq!(2, v.arguments.torrents.len());
        for torrent in v.arguments.torrents {
            match torrent.id {
                Some(TorrentId::Id(Id::Id(7))) => {
                    assert_eq!(Some("Ubuntu x86_64 DVD"), torrent.name.as_deref());
                    assert_eq!(Some(9923890123), torrent.total_size);
                }
                Some(TorrentId::Id(Id::Id(10))) => {
                    assert_eq!(Some("Fedora x86_64 DVD"), torrent.name.as_deref());
                    assert_eq!(Some(34983493932), torrent.total_size);
                }
                id => panic!("Unknown torrent id: {id:?}"),
            }
        }
    }
}
