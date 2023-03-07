use modality_api::AttrVal;
use modality_reflector_config::{Config, TomlValue, TopLevelIngest, CONFIG_ENV_VAR};
use serde::Deserialize;
use std::{
    env,
    path::{Path, PathBuf},
    str::FromStr,
};
use url::Url;
use uuid::Uuid;

use crate::{
    auth::{AuthTokenBytes, AuthTokenError},
    prelude::ReflectorOpts,
};

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct JsonConfig {
    pub auth_token: Option<String>,
    pub ingest: TopLevelIngest,
    pub plugin: PluginConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct PluginConfig {
    pub run_id: Option<Uuid>,

    // TODO this is currently one-attr, with fallbacks. Should it instead be compound?
    /// The json path to the key that will be used to determine the
    /// name of an event. If given multiple times, the paths with be
    /// checked in order and the first json path which exists will be
    /// used.
    pub event_names: Vec<String>,

    pub event_name_prefix: Option<String>,

    // TODO this is currently one-attr, with fallbacks. Should it instead be compound?
    /// The json path to the key that will be used to determine the
    /// name (and identity) of a timeline. If given multiple times,
    /// the paths with be checked in order and the first json path
    /// which exists will be used. The chosen key and value will be
    /// added to the timeline attrs.
    pub timeline_names: Vec<String>,

    /// Add this string as a prefix to each generated timeline name
    pub timeline_name_prefix: Option<String>,

    /// A json path to to add as a timeline attribute.
    pub timeline_attrs: Vec<String>,

    /// Rename a timeline attribute key as it is being imported
    pub rename_timeline_attrs: Vec<AttrKeyRename>,

    /// Rename an event attribute key as it is being imported
    pub rename_event_attrs: Vec<AttrKeyRename>,

    /// The json path where the event's timestamp can be found
    pub timestamp_attr: Option<String>,

    /// The units of timestamp_attr, in the source data. One of s, ms, us, ns.
    pub timestamp_attr_units: Option<TimestampUnit>,

    /// If we see a line that doesn't look like a json object, parse it with this regex
    pub non_json_regex: Option<String>,

    /// The name for an attr to use for data extracted from subgroups
    /// in non-json-regex. These are treated positionally, with
    /// respect to the subgroup's position in the regex. This data is
    /// produced early, so all the other options apply equally to the
    /// regex-extracted data and to the json-sourced data.
    pub non_json_attrs: Vec<String>,

    #[serde(flatten)]
    pub import: ImportConfig,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct AttrKeyRename {
    /// The attr key to rename
    pub original: String,

    /// The new attr key name to use
    pub new: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "kebab-case", default)]
pub struct ImportConfig {
    pub inputs: Vec<PathBuf>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TimestampUnit {
    Seconds,
    Milliseconds,
    Microseconds,
    Nanoseconds,
}

impl TimestampUnit {
    pub fn attr_val_to_ns(
        &self,
        v: &modality_api::AttrVal,
    ) -> Result<AttrVal, Box<dyn std::error::Error>> {
        let float_val = match v {
            AttrVal::Integer(i) => *i as f64,
            AttrVal::BigInt(i) => *i.as_ref() as f64,
            AttrVal::Float(of) => of.0,
            _ => return Err(format!("Found non-numeric value in timestamp field: {v}").into()),
        };

        let float_ns = float_val * self.to_ns_factor();
        Ok(modality_api::BigInt::new_attr_val(float_ns as i128))
    }

    pub fn to_ns_factor(&self) -> f64 {
        match self {
            TimestampUnit::Seconds => 1_000_000_000.0,
            TimestampUnit::Milliseconds => 1_000_000.0,
            TimestampUnit::Microseconds => 1_000.0,
            TimestampUnit::Nanoseconds => 1.0,
        }
    }
}

impl Default for TimestampUnit {
    fn default() -> Self {
        TimestampUnit::Nanoseconds
    }
}

impl FromStr for TimestampUnit {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_ref() {
            "s" | "secs" | "seconds" => Ok(TimestampUnit::Seconds),
            "ms" | "millis" | "milliseconds" => Ok(TimestampUnit::Milliseconds),
            "us" | "micros" | "microseconds" => Ok(TimestampUnit::Microseconds),
            "ns" | "nanos" | "nanoseconds" => Ok(TimestampUnit::Nanoseconds),
            _ => Err(format!("Unknown time unit {s}")),
        }
    }
}

impl<'de> Deserialize<'de> for TimestampUnit {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        FromStr::from_str(&s).map_err(serde::de::Error::custom)
    }
}

impl JsonConfig {
    pub fn load_merge_with_opts(
        rf_opts: ReflectorOpts,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let cfg = if let Some(cfg_path) = &rf_opts.config_file {
            modality_reflector_config::try_from_file(cfg_path)?
        } else if let Ok(env_path) = env::var(CONFIG_ENV_VAR) {
            modality_reflector_config::try_from_file(Path::new(&env_path))?
        } else {
            Config::default()
        };

        let mut ingest = cfg.ingest.clone().unwrap_or_default();
        if let Some(url) = &rf_opts.protocol_parent_url {
            ingest.protocol_parent_url = Some(url.clone());
        }
        if rf_opts.allow_insecure_tls {
            ingest.allow_insecure_tls = true;
        }

        let mut plugin: PluginConfig =
            TomlValue::Table(cfg.metadata.into_iter().collect()).try_into()?;
        if let Some(run_id) = rf_opts.run_id {
            plugin.run_id = Some(run_id);
        }

        Ok(Self {
            auth_token: rf_opts.auth_token,
            ingest,
            plugin,
        })
    }

    pub fn protocol_parent_url(&self) -> Result<Url, url::ParseError> {
        if let Some(url) = &self.ingest.protocol_parent_url {
            Ok(url.clone())
        } else {
            let url = Url::parse("modality-ingest://127.0.0.1:14188")?;
            Ok(url)
        }
    }

    pub fn resolve_auth(&self) -> Result<AuthTokenBytes, AuthTokenError> {
        AuthTokenBytes::resolve(self.auth_token.as_deref())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use modality_reflector_config::{AttrKeyEqValuePair, TimelineAttributes};
    use pretty_assertions::assert_eq;
    use std::str::FromStr;
    use std::{env, fs::File, io::Write};

    const IMPORT_CONFIG: &str = r#"[ingest]
protocol-parent-url = 'modality-ingest://127.0.0.1:14182'
additional-timeline-attributes = [
    "ci_run=1",
    "module='linux-import'",
]

[metadata]
run-id = 'a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d1'
trace-uuid = 'a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d2'
log-level = 'info'
trace-name = 'my-trace'
clock-class-offset-ns = -1
clock-class-offset-s = 2
force-clock-class-origin-unix-epoch = true
inputs = ['path/traces-a', 'path/traces-b']
"#;

    const LTTNG_LIVE_CONFIG: &str = r#"[ingest]
protocol-parent-url = 'modality-ingest://127.0.0.1:14182'
additional-timeline-attributes = [
    "ci_run=1",
    "module='linux-import'",
]

[metadata]
run-id = 'a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d1'
trace-uuid = 'a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d2'
log-level = 'debug'
retry-duration-us = 100
session-not-found-action = 'end'
url = 'net://localhost/host/ubuntu-focal/my-kernel-session'
"#;

    #[test]
    fn import_cfg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my_config.toml");
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(IMPORT_CONFIG.as_bytes()).unwrap();
            f.flush().unwrap();
        }

        let cfg = JsonConfig::load_merge_with_opts(
            ReflectorOpts {
                config_file: Some(path.to_path_buf()),
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();

        env::set_var(CONFIG_ENV_VAR, path);
        let env_cfg =
            JsonConfig::load_merge_with_opts(Default::default(), Default::default()).unwrap();
        env::remove_var(CONFIG_ENV_VAR);
        assert_eq!(cfg, env_cfg);

        assert_eq!(
            cfg,
            JsonConfig {
                auth_token: None,
                ingest: TopLevelIngest {
                    protocol_parent_url: Url::parse("modality-ingest://127.0.0.1:14182")
                        .unwrap()
                        .into(),
                    allow_insecure_tls: false,
                    protocol_child_port: None,
                    timeline_attributes: TimelineAttributes {
                        additional_timeline_attributes: vec![
                            AttrKeyEqValuePair::from_str("ci_run=1").unwrap(),
                            AttrKeyEqValuePair::from_str("module='linux-import'").unwrap(),
                        ],
                        override_timeline_attributes: Default::default(),
                    },
                    max_write_batch_staleness: None,
                },
                plugin: PluginConfig {
                    run_id: Uuid::from_str("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d1")
                        .unwrap()
                        .into(),
                    trace_uuid: Uuid::from_str("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d2")
                        .unwrap()
                        .into(),
                    log_level: babeltrace2_sys::LoggingLevel::Info.into(),
                    import: ImportConfig {
                        trace_name: "my-trace".to_owned().into(),
                        clock_class_offset_ns: Some(-1_i64),
                        clock_class_offset_s: 2_i64.into(),
                        force_clock_class_origin_unix_epoch: true.into(),
                        inputs: vec![
                            PathBuf::from("path/traces-a"),
                            PathBuf::from("path/traces-b")
                        ],
                    },
                    lttng_live: Default::default(),
                }
            }
        );
    }

    #[test]
    fn lttng_live_cfg() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("my_config.toml");
        {
            let mut f = File::create(&path).unwrap();
            f.write_all(LTTNG_LIVE_CONFIG.as_bytes()).unwrap();
            f.flush().unwrap();
        }

        let cfg = JsonConfig::load_merge_with_opts(
            ReflectorOpts {
                config_file: Some(path.to_path_buf()),
                ..Default::default()
            },
            Default::default(),
        )
        .unwrap();

        env::set_var(CONFIG_ENV_VAR, path);
        let env_cfg =
            JsonConfig::load_merge_with_opts(Default::default(), Default::default()).unwrap();
        env::remove_var(CONFIG_ENV_VAR);
        assert_eq!(cfg, env_cfg);

        assert_eq!(
            cfg,
            JsonConfig {
                auth_token: None,
                ingest: TopLevelIngest {
                    protocol_parent_url: Url::parse("modality-ingest://127.0.0.1:14182")
                        .unwrap()
                        .into(),
                    allow_insecure_tls: false,
                    protocol_child_port: None,
                    timeline_attributes: TimelineAttributes {
                        additional_timeline_attributes: vec![
                            AttrKeyEqValuePair::from_str("ci_run=1").unwrap(),
                            AttrKeyEqValuePair::from_str("module='linux-import'").unwrap(),
                        ],
                        override_timeline_attributes: Default::default(),
                    },
                    max_write_batch_staleness: None,
                },
                plugin: PluginConfig {
                    run_id: Uuid::from_str("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d1")
                        .unwrap()
                        .into(),
                    trace_uuid: Uuid::from_str("a1a2a3a4b1b2c1c2d1d2d3d4d5d6d7d2")
                        .unwrap()
                        .into(),
                    log_level: babeltrace2_sys::LoggingLevel::Debug.into(),
                    import: Default::default(),
                    lttng_live: LttngLiveConfig {
                        retry_duration_us: 100.into(),
                        session_not_found_action: babeltrace2_sys::SessionNotFoundAction::End
                            .into(),
                        url: Url::parse("net://localhost/host/ubuntu-focal/my-kernel-session")
                            .unwrap()
                            .into(),
                    }
                }
            }
        );
    }
}
