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
    pub make: Option<String>,
    pub model: Option<String>,
    pub colour: Option<String>,
    pub team: Option<String>,
    pub race_number: Option<String>,
    pub driver: Option<String>,
    pub country: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LabelArgs {
    #[serde(deserialize_with = "positive")]
    pub detection_id: i64,
    pub stars: i64,
    #[serde(default)]
    pub pan: bool,
}

/// The kinds of subject a sharpness model is trained for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Region {
    #[default]
    Vehicle,
    Person,
    Face,
    Eye,
}

impl Region {
    pub fn as_str(self) -> &'static str {
        match self {
            Region::Vehicle => "vehicle",
            Region::Person => "person",
            Region::Face => "face",
            Region::Eye => "eye",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct RegionArgs {
    #[serde(default)]
    pub region: Region,
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
    Identify(JobArgs),
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
    TrainingStatus {},
    TrainLabel(LabelArgs),
    UndoLabel {},
    TrainModel(RegionArgs),
    ForgetModel(RegionArgs),
    TrainTaste {},
    // Album operations (see rust/API.md).
    Rescore(JobArgs),
    PickKeepers(JobArgs),
    Group(JobArgs),
    /// The old name for `group`, kept so an older window still works.
    Regroup(JobArgs),
    BulkEdit(BulkArgs),
    RenameJob(RenameArgs),
    Summary(JobArgs),
    Cover(JobArgs),
    Filling(JobArgs),
    CacheInfo {},
    CacheClear(CacheClearArgs),
    ResetIdentifications(AlbumScope),
    ResetDetections(AlbumScope),
    ResetAll {},
    CheckUpdate {},
    InstallUpdate {},
    WatchStatus {},
    SetWatch(WatchArgs),
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
    fn an_edit_carries_the_python_fields_and_keeps_its_defaults() {
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

    #[test]
    fn a_region_defaults_to_vehicle_and_rejects_unknown_ones() {
        let Ok(Command::TrainModel(a)) = Command::parse("train_model", json!({})) else {
            panic!()
        };
        assert_eq!(a.region, Region::Vehicle);
        let Ok(Command::ForgetModel(a)) = Command::parse("forget_model", json!({"region": "eye"}))
        else {
            panic!()
        };
        assert_eq!(a.region.as_str(), "eye");
        assert!(Command::parse("train_model", json!({"region": "cat"})).is_err());
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
