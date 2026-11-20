//! The engine's command surface, typed.
//!
//! The frontend sends `{action, args}`. It is parsed once, here, into a
//! [`Command`], so a missing or malformed argument is one clear error and every
//! handler receives what it needs rather than a bag of JSON.

use serde::{Deserialize, Deserializer};
use serde_json::{Map, Value};

fn positive<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    let n = i64::deserialize(d)?;
    if n > 0 {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("must be a positive id"))
    }
}

fn positive_all<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
    let ids = Vec::<i64>::deserialize(d)?;
    if ids.iter().all(|n| *n > 0) {
        Ok(ids)
    } else {
        Err(serde::de::Error::custom("every id must be positive"))
    }
}

fn positive_opt<'de, D: Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    match Option::<i64>::deserialize(d)? {
        Some(n) if n <= 0 => Err(serde::de::Error::custom("must be a positive id")),
        other => Ok(other),
    }
}

fn yes() -> bool {
    true
}

/// Tells a field that was left out (leave the value alone) from one sent as
/// `null` (clear it): `None` absent, `Some(None)` null, `Some(Some(v))` a value.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Some(Option::<T>::deserialize(d)?))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
}

/// `taste_rank`: score every frame's cached preview, unless it already has a
/// score -- `force` rescores the whole album.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TasteArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageArgs {
    #[serde(deserialize_with = "positive")]
    pub image_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct KeyArgs {
    pub key: String,
}

#[derive(Debug, Deserialize)]
pub struct PlateArgs {
    pub plate: String,
}

/// Start a new album (`root`) or resume an unfinished one (`jobId`).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanArgs {
    #[serde(default)]
    pub stage: ScanStage,
    #[serde(default = "yes")]
    pub recursive: bool,
    pub job_id: Option<i64>,
    pub root: Option<String>,
    pub profile: Option<String>,
    pub label: Option<String>,
    pub read_plates: Option<bool>,
    pub read_numbers: Option<bool>,
    /// Motorsport: look at people too. Left out keeps the saved default.
    pub include_people: Option<bool>,
    /// The shoot type for a *new* album: stored on that album's own settings,
    /// never written back to the global defaults.
    pub scan_profile: Option<String>,
    /// Blur strictness for a new album: `strict`, `moderate` or `lenient`.
    pub blur_strictness: Option<String>,
    /// Burst gap in seconds for a new album.
    pub burst_gap: Option<f64>,
    /// Grouping for a new album: every frame its own stack, no keeper.
    #[serde(default)]
    pub grouping_stacks: Option<bool>,
    /// Grouping for a new album: "off", "gentle", "normal" or "eager".
    #[serde(default)]
    pub grouping_sensitivity: Option<String>,
    /// Grouping for a new album: seconds that count as a pause (0 disables it).
    #[serde(default)]
    pub grouping_pause: Option<f64>,
    #[serde(default)]
    pub grouping_keep_pans: Option<bool>,
    /// Grouping for a new album: join look-alike neighbouring bursts a few
    /// seconds apart.
    #[serde(default)]
    pub grouping_merge: Option<bool>,
    #[serde(default)]
    pub grouping_focus_anchor: Option<bool>,
    /// What a *new* album's scan should do: "both", "cull" or "identify".
    /// Left out (as a resume does) keeps the saved default.
    #[serde(default)]
    pub workflow_mode: Option<String>,
    /// What a *new* album's identify lane works on: "keepers" (the effective
    /// keeper -- a hand pick, or the engine's suggestion when the burst has no
    /// hand pick; the default) or "everything". Left out (as a resume does)
    /// keeps the saved default.
    #[serde(default)]
    pub identify_scope: Option<String>,
    /// Detector and reader switches for the *new* album (`profile::Feature`
    /// keys to booleans); a missing key follows the shoot type.
    #[serde(default)]
    pub overrides: Option<Map<String, Value>>,
}

/// Identify an album. `scope` overrides the album's `identify_scope` for this
/// run only: "keepers" or "everything".
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentifyArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    #[serde(default)]
    pub scope: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkArgs {
    #[serde(deserialize_with = "positive")]
    pub image_id: i64,
    /// 0-5 to set, `null` to clear, absent to leave alone.
    #[serde(default, deserialize_with = "present")]
    pub stars: Option<Option<i64>>,
    pub rejected: Option<bool>,
    pub picked: Option<bool>,
}

/// Hand edits to one vehicle's fields; a field left out is untouched, `null` clears it.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditArgs {
    #[serde(deserialize_with = "positive")]
    pub detection_id: i64,
    #[serde(default, deserialize_with = "present")]
    pub make: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub model: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub colour: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub team: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub driver: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub country: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    pub person_name: Option<Option<String>>,
    /// Digits only once stored; `number` is accepted as the Python name for it.
    #[serde(
        default,
        deserialize_with = "present",
        alias = "number",
        alias = "race_number"
    )]
    pub race_number: Option<Option<String>>,
    /// Letters and digits, upper-cased, once stored.
    #[serde(default, deserialize_with = "present")]
    pub plate: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[serde(alias = "plate_state")]
    pub plate_state: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    #[serde(alias = "body_type")]
    pub body_type: Option<Option<String>>,
    /// The whole list as it should now stand; blanks and repeats are dropped.
    #[serde(default, deserialize_with = "present")]
    pub sponsors: Option<Option<Vec<String>>>,
    pub rejected: Option<bool>,
    pub bystander: Option<bool>,
    /// A hand edit counts as a review unless it says otherwise.
    #[serde(default = "yes")]
    pub reviewed: bool,
    /// 1-5 sets the hand star; 0 or `null` hands the frame back to the measured
    /// rating; absent leaves it. (`mark` differs: there 0 is a deliberate zero.)
    #[serde(default, deserialize_with = "present")]
    pub stars: Option<Option<i64>>,
}

/// One change applied to many detections at once.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BulkArgs {
    #[serde(deserialize_with = "positive_all")]
    pub ids: Vec<i64>,
    /// Digits are kept; an empty result clears the number.
    pub number: Option<String>,
    pub rejected: Option<bool>,
    pub bystander: Option<bool>,
    pub reviewed: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    /// Blank or `null` goes back to the folder name.
    pub label: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncludePeopleArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    pub value: bool,
}

/// What Conrod will do for a shoot type: the summary the type picker shows.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetPolicyArgs {
    pub preset: String,
    /// The album's (or sheet's) own switches, by feature key.
    #[serde(default)]
    pub overrides: Map<String, Value>,
    #[serde(default)]
    pub include_people: bool,
}

/// Switch one detector or reader on or off for an album; `value: null`
/// goes back to the shoot type's default.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    pub feature: String,
    #[serde(default)]
    pub value: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateJobSettingsArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    pub patch: Map<String, Value>,
}

/// One album, or every album when `jobId` is left out.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AlbumScope {
    #[serde(default, deserialize_with = "positive_opt")]
    pub job_id: Option<i64>,
}

/// What to drop from the cache; nothing is dropped unless asked for.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheClearArgs {
    /// Cached files that belong to no album any more.
    #[serde(default)]
    pub orphaned: bool,
    /// Every full-size preview; they are pulled from the originals again on demand.
    #[serde(default)]
    pub previews: bool,
    /// The previews of this one album only.
    #[serde(default, deserialize_with = "positive_opt")]
    pub job_id: Option<i64>,
}

impl EditArgs {
    /// The attribute fields that were sent, with their new value (`None` clears).
    pub fn changes(&self) -> Vec<(&'static str, Option<&str>)> {
        [
            ("make", &self.make),
            ("model", &self.model),
            ("colour", &self.colour),
            ("team", &self.team),
            ("driver", &self.driver),
            ("country", &self.country),
            ("person_name", &self.person_name),
            ("race_number", &self.race_number),
            ("plate", &self.plate),
            ("plate_state", &self.plate_state),
            ("body_type", &self.body_type),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.as_ref().map(|v| (key, v.as_deref())))
        .collect()
    }
}

#[derive(Debug, Deserialize)]
pub struct KnownArgs {
    pub plate: String,
    #[serde(default)]
    pub old_plate: Option<String>,
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub team: Option<String>,
    pub race_number: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
}

/// `provider_models`: ask a provider which models its key can call.
#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelsArgs {
    pub provider: String,
    #[serde(default)]
    pub host: String,
    #[serde(default)]
    pub api_key: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct OllamaModelsArgs {
    #[serde(default)]
    pub host: Option<String>,
}

/// What a bench item is claimed to be; a blank field is skipped when scoring.
#[derive(Debug, Deserialize, Default)]
pub struct BenchTruth {
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub number: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BenchItem {
    pub path: String,
    pub sharp: bool,
    #[serde(default)]
    pub truth: BenchTruth,
}

#[derive(Debug, Deserialize)]
pub struct BenchModel {
    /// `VlmService::id` of the server to run on.
    pub service_id: String,
    /// The model to run on that server.
    pub model: String,
}

#[derive(Debug, Deserialize)]
pub struct BenchRunArgs {
    pub items: Vec<BenchItem>,
    pub models: Vec<BenchModel>,
}

/// `lightroom_pending`: which bridge folder to list. `in` is the default.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightroomPendingArgs {
    pub dir: Option<String>,
}

/// `lightroom_import`: the plugin's packages, or an `.lrcat` path instead.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LightroomImportArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    /// An absolute path to a Lightroom Classic catalog, read read-only.
    #[serde(default)]
    pub catalog: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "action", content = "args", rename_all = "snake_case")]
pub enum Command {
    Bootstrap {},
    Health {},
    Status {},
    Jobs {},
    Review(JobArgs),
    Scan(ScanArgs),
    Pause {},
    ResumeScan {},
    Stop {},
    DeleteJob(JobArgs),
    Identify(IdentifyArgs),
    Identified(JobArgs),
    ProviderModels(ProviderModelsArgs),
    Write(WriteArgs),
    CancelOperation(KeyArgs),
    InstallModels {},
    SaveSettings(Map<String, Value>),
    Mark(MarkArgs),
    EditDetection(EditArgs),
    Preview(ImageArgs),
    Known {},
    ExportKnown {},
    ImportKnown(CsvArgs),
    SeedKnown(AlbumScope),
    ImportEntries(CsvArgs),
    SaveKnown(KnownArgs),
    DeleteKnown(PlateArgs),
    DeleteAllKnown {},
    // Album operations (see API.md).
    Rescore(JobArgs),
    Rescan(JobArgs),
    PickKeepers(JobArgs),
    /// Read the camera's autofocus area for every RAW of the album, then rate
    /// and group the frames the detector missed by it.
    FocusPoints(JobArgs),
    /// Run the detector again over the frames it found nothing in, at a one-off
    /// lower bar, for the subjects it missed.
    Redetect(JobArgs),
    /// Measure each frame's composition -- where the subject sits, how the
    /// frame balances, how busy the background is and whether a leading line
    /// runs into it -- from its thumbnail.
    Composition(JobArgs),
    /// Hide the extra imports of one shot (the same frame under two or three
    /// names) from bursts, picks, counts and writes.
    CollapseCopies(JobArgs),
    /// Show the copies `collapse_copies` hid again, then regroup the album.
    RestoreCopies(JobArgs),
    /// Score every frame's cached preview with the learned taste head. A sort
    /// signal only: it never changes a pick.
    TasteRank(TasteArgs),
    Group(JobArgs),
    /// The old name for `group`, kept so an older window still works.
    Regroup(JobArgs),
    BulkEdit(BulkArgs),
    RenameJob(RenameArgs),
    UpdateJobSettings(UpdateJobSettingsArgs),
    SetIncludePeople(IncludePeopleArgs),
    /// What Conrod will do for a shoot type, with the given overrides.
    PresetPolicy(PresetPolicyArgs),
    /// Per-album detector / reader switch.
    SetFeature(FeatureArgs),
    Summary(JobArgs),
    /// The shoot's own statistics card: what was used, how it was exposed,
    /// what the cull measured. Backfills an old album's shot columns first.
    ShootStats(JobArgs),
    /// The same numbers for the whole library: every album's frames, space,
    /// keepers and quality, with gear and exposure only from the albums whose
    /// frames all record them.
    GlobalStats {},
    Cover(JobArgs),
    Filling(JobArgs),
    CacheInfo {},
    CacheClear(CacheClearArgs),
    ResetIdentifications(AlbumScope),
    ResetDetections(AlbumScope),
    ResetRatings(AlbumScope),
    /// Return one album to the state just after import: every detection and
    /// measurement goes, the frames stay and go back to pending. The job id is
    /// required (one album only), unlike the other resets.
    ResetAlbum(AlbumScope),
    ResetAll {},
    OllamaModels(OllamaModelsArgs),
    WatchStatus {},
    SetWatch(WatchArgs),
    BenchPick {},
    BenchRun(BenchRunArgs),
    /// Per-stage worker counts, their documented caps and whether a change
    /// applies live (none do: every pool is sized when the scan starts).
    WorkerLimits {},
    DescribeImage {
        path: String,
    },
    /// Write the album's verdicts to `bridge\out` for the Conrod Bridge plugin.
    LightroomExport(JobArgs),
    /// Packages waiting in the bridge (`dir`: "in" by default, or "out").
    LightroomPending(LightroomPendingArgs),
    /// Apply Lightroom's flags from `bridge\in`, or from a `.lrcat` when given.
    LightroomImport(LightroomImportArgs),
}

impl Command {
    /// Parse the wire format. A missing `args` is the same as none.
    pub fn parse(action: &str, args: Value) -> Result<Command, String> {
        let args = if args.is_null() {
            Value::Object(Map::new())
        } else {
            args
        };
        serde_json::from_value(serde_json::json!({"action": action, "args": args}))
            .map_err(|e| format!("{action}: {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_lightroom_bridge_commands_parse_their_arguments() {
        let Ok(Command::LightroomExport(a)) =
            Command::parse("lightroom_export", json!({"jobId": 7}))
        else {
            panic!()
        };
        assert_eq!(a.job_id, 7);
        assert!(Command::parse("lightroom_export", json!({})).is_err());
        assert!(Command::parse("lightroom_export", json!({"jobId": 0})).is_err());

        let Ok(Command::LightroomPending(a)) = Command::parse("lightroom_pending", json!({}))
        else {
            panic!()
        };
        assert_eq!(a.dir, None, "the reader defaults to in");
        let Ok(Command::LightroomPending(a)) =
            Command::parse("lightroom_pending", json!({"dir": "out"}))
        else {
            panic!()
        };
        assert_eq!(a.dir.as_deref(), Some("out"));

        let Ok(Command::LightroomImport(a)) =
            Command::parse("lightroom_import", json!({"jobId": 7}))
        else {
            panic!()
        };
        assert_eq!((a.job_id, a.catalog), (7, None));
        let Ok(Command::LightroomImport(a)) = Command::parse(
            "lightroom_import",
            json!({"jobId": 7, "catalog": "C:\\Photos\\Lightroom.lrcat"}),
        ) else {
            panic!()
        };
        assert_eq!(a.catalog.as_deref(), Some("C:\\Photos\\Lightroom.lrcat"));
        assert!(Command::parse("lightroom_import", json!({"jobId": 0})).is_err());
    }

    #[test]
    fn the_wire_format_parses_and_bad_arguments_say_which() {
        assert!(matches!(
            Command::parse("status", json!({})),
            Ok(Command::Status {})
        ));
        assert!(matches!(
            Command::parse("status", Value::Null),
            Ok(Command::Status {})
        ));
        let Ok(Command::Review(a)) = Command::parse("review", json!({"jobId": 4})) else {
            panic!()
        };
        assert_eq!(a.job_id, 4);
        let err = Command::parse("review", json!({"jobId": 0})).unwrap_err();
        assert!(err.contains("review") && err.contains("positive"), "{err}");
        assert!(Command::parse("nope", json!({}))
            .unwrap_err()
            .contains("nope"));
        assert!(Command::parse("mark", json!({}))
            .unwrap_err()
            .contains("imageId"));
        let Ok(Command::Scan(scan)) = Command::parse(
            "scan",
            json!({"root":"C:\\\\Photos", "readPlates":false, "readNumbers":true}),
        ) else {
            panic!()
        };
        assert_eq!(
            (scan.read_plates, scan.read_numbers),
            (Some(false), Some(true))
        );
        // The library-wide card takes no arguments, like `status`.
        assert!(matches!(
            Command::parse("global_stats", json!({})),
            Ok(Command::GlobalStats {})
        ));
        assert!(matches!(
            Command::parse("global_stats", Value::Null),
            Ok(Command::GlobalStats {})
        ));
    }

    #[test]
    fn a_scan_carries_the_sheets_grouping_choices_in_camel_case() {
        let Ok(Command::Scan(scan)) = Command::parse(
            "scan",
            json!({
                "root": "C:\\\\Photos",
                "groupingStacks": false,
                "groupingSensitivity": "eager",
                "groupingPause": 0.0,
                "groupingKeepPans": false,
                "groupingMerge": false,
                "groupingFocusAnchor": false,
            }),
        ) else {
            panic!()
        };
        assert_eq!(scan.grouping_stacks, Some(false));
        assert_eq!(scan.grouping_sensitivity.as_deref(), Some("eager"));
        assert_eq!(scan.grouping_pause, Some(0.0));
        assert_eq!(scan.grouping_keep_pans, Some(false));
        assert_eq!(scan.grouping_merge, Some(false));
        assert_eq!(scan.grouping_focus_anchor, Some(false));

        // The sheet may send none of them: absent is None, not a default.
        let Ok(Command::Scan(scan)) = Command::parse(
            "scan",
            json!({"root": "C:\\\\Photos", "groupingSensitivity": "off"}),
        ) else {
            panic!()
        };
        assert_eq!(scan.grouping_sensitivity.as_deref(), Some("off"));
        assert_eq!(scan.grouping_stacks, None);
        assert_eq!(scan.grouping_pause, None);
        assert_eq!(scan.grouping_keep_pans, None);
        assert_eq!(scan.grouping_merge, None);
        assert_eq!(scan.grouping_focus_anchor, None);
        // Settings keys are snake_case on the wire only as JSON patch keys; a
        // scan takes the camelCase arg names.
        let Ok(Command::Scan(scan)) = Command::parse(
            "scan",
            json!({"root": "C:\\\\Photos", "grouping_pause": 1.0}),
        ) else {
            panic!()
        };
        assert_eq!(scan.grouping_pause, None);
    }

    #[test]
    fn a_scan_carries_the_albums_workflow_mode_in_camel_case() {
        for word in ["both", "cull", "identify"] {
            let Ok(Command::Scan(scan)) = Command::parse(
                "scan",
                json!({"root": "C:\\\\Photos", "workflowMode": word}),
            ) else {
                panic!()
            };
            assert_eq!(scan.workflow_mode.as_deref(), Some(word));
        }
        // Missing keeps None: a new album then keeps the stored default.
        let Ok(Command::Scan(scan)) = Command::parse("scan", json!({"root": "C:\\\\Photos"}))
        else {
            panic!()
        };
        assert_eq!(scan.workflow_mode, None);
        // The settings key spelling is not a scan arg.
        let Ok(Command::Scan(scan)) = Command::parse(
            "scan",
            json!({"root": "C:\\\\Photos", "workflow_mode": "cull"}),
        ) else {
            panic!()
        };
        assert_eq!(scan.workflow_mode, None);
    }

    #[test]
    fn a_scan_carries_the_albums_identify_scope_in_camel_case() {
        for word in ["keepers", "everything"] {
            let Ok(Command::Scan(scan)) = Command::parse(
                "scan",
                json!({"root": "C:\\\\Photos", "identifyScope": word}),
            ) else {
                panic!()
            };
            assert_eq!(scan.identify_scope.as_deref(), Some(word));
        }
        // Missing keeps None: a new album then keeps the stored default.
        let Ok(Command::Scan(scan)) = Command::parse("scan", json!({"root": "C:\\\\Photos"}))
        else {
            panic!()
        };
        assert_eq!(scan.identify_scope, None);
        // The settings key spelling is not a scan arg.
        let Ok(Command::Scan(scan)) = Command::parse(
            "scan",
            json!({"root": "C:\\\\Photos", "identify_scope": "everything"}),
        ) else {
            panic!()
        };
        assert_eq!(scan.identify_scope, None);

        // The identify command's own scope is optional, and absent is None.
        let Ok(Command::Identify(a)) = Command::parse("identify", json!({"jobId": 3})) else {
            panic!()
        };
        assert_eq!(a.job_id, 3);
        assert_eq!(a.scope, None);
        let Ok(Command::Identify(a)) =
            Command::parse("identify", json!({"jobId": 3, "scope": "everything"}))
        else {
            panic!()
        };
        assert_eq!(a.scope.as_deref(), Some("everything"));
    }

    #[test]
    fn absent_null_and_a_value_are_three_different_things() {
        let Ok(Command::Mark(a)) = Command::parse("mark", json!({"imageId": 1})) else {
            panic!()
        };
        assert_eq!(a.stars, None);
        let Ok(Command::Mark(a)) = Command::parse("mark", json!({"imageId": 1, "stars": null}))
        else {
            panic!()
        };
        assert_eq!(a.stars, Some(None));
        let Ok(Command::Mark(a)) = Command::parse("mark", json!({"imageId": 1, "stars": 0})) else {
            panic!()
        };
        assert_eq!(a.stars, Some(Some(0)));

        let Ok(Command::EditDetection(a)) = Command::parse(
            "edit_detection",
            json!({"detectionId": 2, "plate": "ABC123", "team": null}),
        ) else {
            panic!()
        };
        assert_eq!(a.changes(), vec![("team", None), ("plate", Some("ABC123"))]);
    }

    #[test]
    fn an_edit_carries_the_legacy_field_names_and_keeps_its_defaults() {
        let Ok(Command::EditDetection(a)) = Command::parse(
            "edit_detection",
            json!({"detectionId": 3, "number": "#12", "sponsors": ["Shell", " shell ", ""], "stars": 0}),
        ) else {
            panic!()
        };
        assert_eq!(a.race_number, Some(Some("#12".into())));
        assert_eq!(a.stars, Some(Some(0)));
        assert!(a.reviewed, "a hand edit is a review unless it says not");
        assert_eq!(a.rejected, None);
        assert_eq!(a.sponsors.as_ref().unwrap().as_ref().unwrap().len(), 3);
        let Ok(Command::EditDetection(a)) = Command::parse(
            "edit_detection",
            json!({"detectionId": 3, "stars": null, "reviewed": false, "bystander": true}),
        ) else {
            panic!()
        };
        assert_eq!(a.stars, Some(None));
        assert!(!a.reviewed);
        assert_eq!(a.bystander, Some(true));
    }

    #[test]
    fn album_operations_parse_and_refuse_bad_ids() {
        let Ok(Command::BulkEdit(a)) = Command::parse(
            "bulk_edit",
            json!({"ids": [4, 9], "number": "77", "rejected": true}),
        ) else {
            panic!()
        };
        assert_eq!(
            (a.ids, a.number.as_deref(), a.rejected),
            (vec![4, 9], Some("77"), Some(true))
        );
        assert!(Command::parse("bulk_edit", json!({"ids": [4, 0]})).is_err());
        assert!(Command::parse("bulk_edit", json!({})).is_err());
        assert!(matches!(
            Command::parse("regroup", json!({"jobId": 2})),
            Ok(Command::Regroup(_))
        ));
        assert!(matches!(
            Command::parse("group", json!({"jobId": 2})),
            Ok(Command::Group(_))
        ));
        assert!(Command::parse("rescore", json!({})).is_err());
        assert!(matches!(
            Command::parse("rescan", json!({"jobId": 2})),
            Ok(Command::Rescan(_))
        ));
        assert!(Command::parse("rescan", json!({})).is_err());
        let Ok(Command::TasteRank(taste)) =
            Command::parse("taste_rank", json!({"jobId": 2, "force": true}))
        else {
            panic!()
        };
        assert_eq!((taste.job_id, taste.force), (2, true));
        let Ok(Command::TasteRank(taste)) = Command::parse("taste_rank", json!({"jobId": 2}))
        else {
            panic!()
        };
        assert!(!taste.force, "a plain run only fills in what is missing");
        assert!(Command::parse("taste_rank", json!({})).is_err());
        let Ok(Command::RenameJob(a)) =
            Command::parse("rename_job", json!({"jobId": 1, "label": null}))
        else {
            panic!()
        };
        assert_eq!(a.label, None);
    }

    #[test]
    fn resets_and_cache_clears_default_to_nothing_and_everything_respectively() {
        let Ok(Command::ResetDetections(a)) = Command::parse("reset_detections", json!({})) else {
            panic!()
        };
        assert_eq!(a.job_id, None);
        let Ok(Command::ResetIdentifications(a)) =
            Command::parse("reset_identifications", json!({"jobId": 5}))
        else {
            panic!()
        };
        assert_eq!(a.job_id, Some(5));
        assert!(Command::parse("reset_detections", json!({"jobId": 0})).is_err());
        let Ok(Command::ResetAlbum(a)) = Command::parse("reset_album", json!({"jobId": 7})) else {
            panic!()
        };
        assert_eq!(a.job_id, Some(7), "the album is the whole point");
        assert!(Command::parse("reset_album", json!({"jobId": 0})).is_err());
        let Ok(Command::ResetAlbum(a)) = Command::parse("reset_album", json!({})) else {
            panic!()
        };
        assert_eq!(a.job_id, None, "the handler refuses a missing album");
        let Ok(Command::CacheClear(a)) = Command::parse("cache_clear", json!({})) else {
            panic!()
        };
        assert!(!a.orphaned && !a.previews && a.job_id.is_none());
        let Ok(Command::CacheClear(a)) =
            Command::parse("cache_clear", json!({"orphaned": true, "jobId": 2}))
        else {
            panic!()
        };
        assert!(a.orphaned && !a.previews && a.job_id == Some(2));
        assert!(matches!(
            Command::parse("reset_all", Value::Null),
            Ok(Command::ResetAll {})
        ));
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WatchArgs {
    pub active: bool,
    #[serde(default, deserialize_with = "positive_opt")]
    pub job_id: Option<i64>,
    pub path: Option<String>,
    pub recursive: Option<bool>,
    pub interval: Option<f64>,
}

#[derive(Debug, Deserialize)]
pub struct CsvArgs {
    pub csv: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ScanStage {
    Index,
    #[default]
    Cull,
    Identify,
    All,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteArgs {
    #[serde(deserialize_with = "positive")]
    pub job_id: i64,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub embed_in_raw: bool,
}
