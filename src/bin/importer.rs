#![deny(warnings, clippy::all)]
#![allow(unused)]

use clap::Parser;
use fxhash::FxHashMap;
use itertools::Itertools;
use modality_api::types::TimelineId;
use modality_api::{AttrKey, AttrVal, BigInt};
use modality_ingest_client::IngestClient;
use modality_json::config::{AttrKeyRename, TimestampUnit};
use modality_json::{prelude::*, tracing::try_init_tracing_subscriber};
use regex::Regex;
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::str::FromStr;
use thiserror::Error;
use tracing::{error, warn};

/// Import CTF trace data from files
#[derive(Parser, Debug, Clone)]
#[clap(version)]
pub struct Opts {
    #[clap(flatten)]
    pub rf_opts: ReflectorOpts,

    /// The json path to the key that will be used to determine the
    /// name of an event. If given multiple times, the paths with be
    /// checked in order and the first json path which exists will be
    /// used.
    #[clap(
        long = "event-name",
        name = "event-name",
        help_heading = "IMPORT CONFIGURATION"
    )]
    pub event_names: Vec<String>,

    /// The json path to the key that will be used to determine the
    /// name (and identity) of a timeline. If given multiple times,
    /// the paths with be checked in order and the first json path
    /// which exists will be used. The chosen key and value will be
    /// added to the timeline attrs.
    #[clap(
        long = "timeline-name",
        name = "timeline-name",
        help_heading = "IMPORT CONFIGURATION"
    )]
    pub timeline_names: Vec<String>,

    /// Add this string as a prefix to each generated timeline name
    #[clap(long, help_heading = "IMPORT CONFIGURATION")]
    pub timeline_name_prefix: Option<String>,

    /// A json path to to add as a timeline attribute.
    #[clap(
        long = "timeline-attr",
        name = "timeline-attr-key",
        help_heading = "IMPORT CONFIGURATION"
    )]
    pub timeline_attrs: Vec<String>,

    /// Perform all input processing, but don't actually do the import.
    #[clap(long, name = "trace-name", help_heading = "IMPORT CONFIGURATION")]
    pub dry_run: bool,

    /// Rename a timeline attribute key as it is being imported. Specify as 'original_key,new_key'
    #[clap(
        long = "rename-timeline-attr",
        name = "original.tl.attr,new.tl.attr",
        help_heading = "IMPORT CONFIGURATION",
        value_parser = parse_attr_key_rename
    )]
    pub rename_timeline_attrs: Vec<AttrKeyRename>,

    /// Rename an event attribute key as it is being imported. Specify as 'original_key,new_key'
    #[clap(
        long = "rename-event-attr",
        name = "original.event.attr,new.event.attr",
        help_heading = "IMPORT CONFIGURATION",
        value_parser = parse_attr_key_rename
    )]
    pub rename_event_attrs: Vec<AttrKeyRename>,

    /// The json path where the event's timestamp can be found
    #[clap(long = "timestamp-attr", help_heading = "IMPORT CONFIGURATION")]
    pub timestamp_attr: Option<String>,

    /// The units of timestamp_attr, in the source data. One of s, ms, us, ns.
    #[clap(
        long = "timestamp-attr-units",
        name = "time-unit",
        help_heading = "IMPORT CONFIGURATION"
    )]
    pub timestamp_attr_units: Option<TimestampUnit>,

    /// If we see a line that doesn't look like a json object, parse it with this regex
    #[clap(
        long = "non-json-regex",
        name = "regex",
        help_heading = "IMPORT CONFIGURATION"
    )]
    pub non_json_regex: Option<String>,

    /// The name for an attr to use for data extracted from subgroupbs
    /// in --non-json-regex. These are treated positionally, with
    /// respect to the subgroup's position in the regex.
    #[clap(
        long = "non-json-attr",
        name = "attr-key",
        help_heading = "IMPORT CONFIGURATION"
    )]
    pub non_json_attrs: Vec<String>,

    /// Path to trace directories
    #[clap(name = "input", help_heading = "IMPORT CONFIGURATION")]
    pub inputs: Vec<PathBuf>,
}

fn parse_attr_key_rename(
    s: &str,
) -> Result<AttrKeyRename, Box<dyn std::error::Error + Send + Sync + 'static>> {
    let pos = s
        .find(',')
        .ok_or_else(|| format!("invalid original,new: no `,` found in `{}`", s))?;
    let original = s[..pos].parse()?;
    let new = s[pos + 1..].parse()?;
    Ok(AttrKeyRename { original, new })
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("At least input JSON file is required.")]
    MissingInputs,
}

#[tokio::main]
async fn main() {
    match do_main().await {
        Ok(()) => (),
        Err(e) => {
            eprintln!("{}", e);
            let mut cause = e.source();
            while let Some(err) = cause {
                eprintln!("Caused by: {err}");
                cause = err.source();
            }
            std::process::exit(exitcode::SOFTWARE);
        }
    }
}

type TimelineNameSig = (AttrKey, AttrVal);

async fn do_main() -> Result<(), Box<dyn std::error::Error>> {
    let opts = Opts::parse();

    try_init_tracing_subscriber()?;

    let intr = Interruptor::new();
    let interruptor = intr.clone();
    ctrlc::set_handler(move || {
        if intr.is_set() {
            // 128 (fatal error signal "n") + 2 (control-c is fatal error signal 2)
            std::process::exit(130);
        } else {
            intr.set();
        }
    })?;

    let mut cfg = JsonConfig::load_merge_with_opts(opts.rf_opts)?;
    cfg.plugin.import.inputs.extend(opts.inputs);
    cfg.plugin.event_names.extend(opts.event_names);
    cfg.plugin.timeline_names.extend(opts.timeline_names);
    cfg.plugin.timeline_attrs.extend(opts.timeline_attrs);

    if opts.timeline_name_prefix.is_some() {
        cfg.plugin.timeline_name_prefix = opts.timeline_name_prefix;
    }

    if opts.timestamp_attr.is_some() {
        cfg.plugin.timestamp_attr = opts.timestamp_attr;
    }

    if opts.timestamp_attr_units.is_some() {
        cfg.plugin.timestamp_attr_units = opts.timestamp_attr_units;
    }

    if opts.non_json_regex.is_some() {
        cfg.plugin.non_json_regex = opts.non_json_regex;
    }

    if !opts.non_json_attrs.is_empty() {
        cfg.plugin.non_json_attrs = opts.non_json_attrs;
    }

    let mut rename_timeline_attrs = opts.rename_timeline_attrs.clone();
    rename_timeline_attrs.extend(cfg.plugin.rename_timeline_attrs.clone());

    let mut rename_event_attrs = opts.rename_event_attrs.clone();
    rename_event_attrs.extend(cfg.plugin.rename_event_attrs.clone());

    for p in cfg.plugin.import.inputs.iter() {
        if !p.exists() {
            warn!("Input path '{}' does not exist", p.display());
        }
    }

    let non_json_re = cfg
        .plugin
        .non_json_regex
        .as_deref()
        .map(|re_str| Regex::new(re_str))
        .transpose()?;
    let non_json_attrs = cfg
        .plugin
        .non_json_attrs
        .iter()
        .map(|k| AttrKey::new(k.clone()))
        .collect::<Vec<_>>();

    let mut known_timelines: FxHashMap<TimelineNameSig, TimelineId> = FxHashMap::default();

    if cfg.plugin.import.inputs.is_empty() {
        error!("No input files provided.");
        return Ok(());
    }

    let mut sent_tl_attrs : FxHashMap<(TimelineId, AttrKey), AttrVal> = Default::default();

    let c =
        IngestClient::connect(&cfg.protocol_parent_url()?, cfg.ingest.allow_insecure_tls).await?;
    let c_authed = c.authenticate(cfg.resolve_auth()?.into()).await?;
    let mut client = Client::new(c_authed, rename_timeline_attrs, rename_event_attrs);

    
    for p in cfg.plugin.import.inputs.iter() {
        /// Use a single ordering counter for each input. This will
        /// likely fail if we get the same timeline from two different
        /// files... which is exactly what we want to happen.
        let mut ordering = 0u128;

        let buf = std::fs::read_to_string(p)?;
        let mut s = buf.as_str();

        let mut extra_kvs = vec![];
        let mut pending_json_vals = vec![];

        loop {
            s = s.trim_start();

            match s.chars().next() {
                None => {
                    break;
                }
                Some(c) => {
                    if c == '[' {
                        let (s_prime, vals) = parse_array(s)?;
                        pending_json_vals.extend(vals);
                        s = s_prime;
                    } else if c == '{' {
                        let (s_prime, val) = parse_obj(s)?;
                        pending_json_vals.push(val);
                        s = s_prime;
                    } else {
                        let (s_prime, kvs) =
                            parse_regex_line(s, non_json_re.as_ref(), &non_json_attrs)?;
                        extra_kvs.extend(kvs);
                        s = s_prime;
                    }
                }
            }

            if !pending_json_vals.is_empty() {
                for val in (&pending_json_vals).into_iter() {
                    let rts = prepare_json_object(val, &extra_kvs, &cfg.plugin, &mut known_timelines)?;
                    client.send_event_on_timeline(rts.timeline_id, rts.timeline_kvs, ordering, rts.event_kvs);
                    ordering += 1;
                }

                pending_json_vals.clear();
                extra_kvs.clear();
            }
        }
    }

    Ok(())
}

fn json_from_str<'de, T: serde::Deserialize<'de>>(
    s: &'de str,
) -> Result<(&str, T), Box<dyn std::error::Error>> {
    let mut de = serde_json::Deserializer::from_str(s);
    let value: T = serde::de::Deserialize::deserialize(&mut de)?;
    let str_read = de.into_reader();

    Ok((&s[str_read.index()..], value))
}

type JsonObject = serde_json::Map<String, serde_json::Value>;

fn parse_array(mut s: &str) -> Result<(&str, Vec<serde_json::Value>), Box<dyn std::error::Error>> {
    let (tail, json) = json_from_str::<serde_json::Value>(s)?;
    let Some(vals) = json.as_array() else {
        return Err("Expected JSON array".into());
    };

    Ok((tail, vals.clone()))
}

fn parse_obj(s: &str) -> Result<(&str, serde_json::Value), Box<dyn std::error::Error>> {
    let (tail, json) = json_from_str::<serde_json::Value>(s)?;
    Ok((tail, json))
}

fn parse_regex_line<'a, 'k>(
    s: &'a str,
    re: Option<&Regex>,
    attrs: &[AttrKey],
) -> Result<(&'a str, Vec<(AttrKey, AttrVal)>), Box<dyn std::error::Error>> {
    let re =
        re.ok_or("Found non-json data. Please supply the '--non-json-regex' option to parse it.")?;

    let line = s.lines().next().ok_or("Can't find line in inputs")?;
    let tail = &s[line.len()..];

    let mut out_attrs = vec![];

    let caps = re
        .captures_iter(line)
        .next()
        .ok_or_else(|| format!("Non-json line did not match the supplied regex.\n{s}"))?;

    // the first capture is always the entire match
    let mut caps = caps.iter().skip(1);

    for eob in attrs.iter().zip_longest(caps) {
        match eob {
            itertools::EitherOrBoth::Both(attr, capture) => {
                let capture = capture.ok_or("Regex capture had no corresponding match")?;
                out_attrs.push((attr.clone(), string_to_attr_val(capture.as_str())));
            }
            itertools::EitherOrBoth::Left(attr) => {
                return Err(format!(
                    "Requested non-json attr '{attr}' has no corresponding regex capature."
                )
                .into());
            }
            itertools::EitherOrBoth::Right(capture) => {
                return Err(format!(
                    "Regex capture {} has no corresponding attr; specify with --non-json-attr",
                    capture.map(|c| c.as_str()).ok_or("<no match>")?
                )
                .into());
            }
        }
    }

    Ok((tail, out_attrs))
}

/// Heuristically try to get a reasonably-typed attr val from this
fn string_to_attr_val(s: &str) -> AttrVal {
    if s.contains('.') {
        if let Ok(f) = s.parse::<f64>() {
            return AttrVal::Float(f.into());
        }
    }

    if let Ok(i) = s.parse::<i128>() {
        return BigInt::new_attr_val(i);
    }

    AttrVal::String(s.into())
}

struct ReadyToSendEvent {
    timeline_id: TimelineId,
    timeline_kvs: Vec<(AttrKey, AttrVal)>,
    event_kvs: Vec<(AttrKey, AttrVal)>,
}

fn prepare_json_object(
    val: &serde_json::Value,
    extra_kvs: &Vec<(AttrKey, AttrVal)>,
    cfg: &PluginConfig,
    known_timelines: &mut HashMap<
        (AttrKey, AttrVal),
        TimelineId,
        std::hash::BuildHasherDefault<fxhash::FxHasher>,
    >,
) -> Result<ReadyToSendEvent, Box<dyn std::error::Error>> {
    let Some(obj) = val.as_object() else  {
        return Err("Expected JSON object at top level, or in array.".into());
    };

    let mut all_kvs = extra_kvs.clone();
    walk_obj(&obj, |key_path, val| {
        let key = AttrKey::new(key_path.join("."));
        if let Some(val) = json_leaf_to_attr_val(val) {
            all_kvs.push((key, val));
        }
    });

    let mut timeline_kvs = vec![];
    let mut event_kvs = vec![];
    for (key, val) in all_kvs.into_iter() {
        if cfg.timeline_names.iter().any(|s| s == key.as_ref()) {
            timeline_kvs.push((key, val));
        } else if cfg.timeline_attrs.iter().any(|s| s == key.as_ref()) {
            timeline_kvs.push((key, val));
        } else {
            event_kvs.push((key, val));
        }
    }

    let mut timeline_name_sig = None;
    let mut timeline_name = cfg
        .timeline_name_prefix
        .clone()
        .unwrap_or_else(|| String::new());

    for name_key in cfg.timeline_names.iter() {
        if let Some((k, v)) = timeline_kvs.iter().find(|(k, _)| k.as_ref() == name_key) {
            timeline_name_sig = Some((k.clone(), v.clone()));
            timeline_name += &v.to_string();
            break;
        }
    }

    let timeline_name_sig = timeline_name_sig.ok_or(
        "Could not determine timeline name and identity for event. \
         Make sure 'timeline-name' is given, and at least one choice is present for each input event."
    )?;

    if !timeline_name.is_empty() {
        timeline_kvs.push((AttrKey::new("name".into()), timeline_name.into()));
    }

    let timeline_id = known_timelines
        .entry(timeline_name_sig)
        .or_insert_with(|| TimelineId::allocate());

    let mut event_name = cfg
        .event_name_prefix
        .clone()
        .unwrap_or_else(|| String::new());
    for name_key in cfg.event_names.iter() {
        if let Some((k, v)) = event_kvs.iter().find(|(k, _)| k.as_ref() == name_key) {
            event_name += &v.to_string();
            break;
        }
    }

    if event_name.is_empty() {
        return Err("Could not determine event name.\
                    Make sure 'event-name' is given, and at least one cohice is present for each input event.".into());
    }
    event_kvs.push((AttrKey::new("name".into()), event_name.into()));

    if let Some(ta) = &cfg.timestamp_attr {
        if let Some((_, val)) = event_kvs.iter().find(|(k, _)| ta == k.as_ref()) {
            let units = cfg.timestamp_attr_units.unwrap_or_default();
            let val = units.attr_val_to_ns(val)?;
            event_kvs.push((AttrKey::new("timestamp".into()), val.clone()));
        }
    }

    let rts = ReadyToSendEvent {
        timeline_id: *timeline_id,
        timeline_kvs,
        event_kvs,
    };

    Ok(rts)
}

fn json_leaf_to_attr_val(val: &serde_json::Value) -> Option<AttrVal> {
    match val {
        // We never call this function with an array or object
        serde_json::Value::Array(_) => None,
        serde_json::Value::Object(_) => None,
        serde_json::Value::Null => None,
        serde_json::Value::Bool(b) => Some(AttrVal::Bool(*b)),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(AttrVal::Integer(i))
            } else if let Some(u) = n.as_u64() {
                Some(BigInt::new_attr_val(u as i128))
            } else if let Some(f) = n.as_f64() {
                Some(AttrVal::Float(f.into()))
            } else {
                // There are just three variants of `Number` in serde-json, and they're handled above.
                unreachable!()
            }
        }
        serde_json::Value::String(s) => Some(AttrVal::String(s.clone())),
    }
}

fn get_val_at_path<'j>(
    path: &str,
    mut obj: &'j serde_json::Map<String, serde_json::Value>,
) -> Option<&'j serde_json::Value> {
    let mut parts = path.split(".").collect::<VecDeque<_>>();
    while parts.len() > 1 {
        let head = parts.pop_front().unwrap();
        obj = obj.get(head)?.as_object()?;
    }

    obj.get(parts[0])
}

type JsonPath<'a> = Vec<Cow<'a, str>>;

/// Do a depth-first traversal of a json object. Call 'f' at every leaf value (non-object, non-array).
fn walk_obj(
    obj: &serde_json::Map<String, serde_json::Value>,
    mut f: impl FnMut(&JsonPath, &serde_json::Value),
) {
    fn walk_obj_rec(
        path: &JsonPath,
        obj: &serde_json::Map<String, serde_json::Value>,
        f: &mut impl FnMut(&JsonPath, &serde_json::Value),
    ) {
        for (k, v) in obj.iter() {
            let mut path = path.clone();
            path.push(Cow::Borrowed(k));
            match v {
                serde_json::Value::Object(o) => {
                    walk_obj_rec(&path, o, f);
                }
                serde_json::Value::Array(a) => {
                    walk_array_rec(&path, a, f);
                }
                _ => {
                    f(&path, v);
                }
            }
        }
    }

    fn walk_array_rec(
        path: &JsonPath,
        array: &[serde_json::Value],
        f: &mut impl FnMut(&JsonPath, &serde_json::Value),
    ) {
        for (i, v) in array.iter().enumerate() {
            let mut path = path.clone();
            path.push(Cow::Owned(format!("{i}")));

            match v {
                serde_json::Value::Object(o) => {
                    walk_obj_rec(&path, o, f);
                }
                serde_json::Value::Array(a) => {
                    walk_array_rec(&path, a, f);
                }
                _ => {
                    f(&path, v);
                }
            }
        }
    }

    walk_obj_rec(&vec![], obj, &mut f)
}
