use clap::Parser;
use std::path::PathBuf;
use url::Url;
use uuid::Uuid;

#[derive(Parser, Debug, Clone, Default)]
pub struct ReflectorOpts {
    /// Use configuration from file
    #[clap(
        long = "config",
        name = "config file",
        env = "MODALITY_REFLECTOR_CONFIG",
        help_heading = "REFLECTOR CONFIGURATION"
    )]
    pub config_file: Option<PathBuf>,

    /// Modality auth token hex string used to authenticate with.
    /// Can also be provide via the MODALITY_AUTH_TOKEN environment variable.
    #[clap(
        long,
        name = "auth-token-hex-string",
        env = "MODALITY_AUTH_TOKEN",
        help_heading = "REFLECTOR CONFIGURATION"
    )]
    pub auth_token: Option<String>,

    /// The modalityd or modality-reflector ingest protocol parent service address
    ///
    /// The default value is `modality-ingest://127.0.0.1:14188`.
    ///
    /// You can talk directly to the default ingest server port with
    /// `--ingest-protocol-parent-url modality-ingest://127.0.0.1:14182`
    #[clap(
        long = "ingest-protocol-parent-url",
        name = "URL",
        help_heading = "REFLECTOR CONFIGURATION"
    )]
    pub protocol_parent_url: Option<Url>,

    /// Allow insecure TLS
    #[clap(
        short = 'k',
        long = "insecure",
        help_heading = "REFLECTOR CONFIGURATION"
    )]
    pub allow_insecure_tls: bool,

    /// Use the provided UUID as the run ID instead of generating a random one
    #[clap(long, name = "run-uuid", help_heading = "REFLECTOR CONFIGURATION")]
    pub run_id: Option<Uuid>,

    /// Ingest client connection timeout seconds (default 1)
    #[clap(
        long,
        name = "timeout-seconds",
        help_heading = "REFLECTOR CONFIGURATION"
    )]
    pub timeout_seconds: Option<u64>,
}
