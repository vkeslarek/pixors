use clap::Parser;
use serde::Deserialize;
use std::path::Path;

// ---- Log level ----

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Default for LogLevel {
    fn default() -> Self {
        if cfg!(debug_assertions) {
            Self::Debug
        } else {
            Self::Info
        }
    }
}

impl From<&LogLevel> for tracing::Level {
    fn from(level: &LogLevel) -> Self {
        match level {
            LogLevel::Error => tracing::Level::ERROR,
            LogLevel::Warn => tracing::Level::WARN,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Trace => tracing::Level::TRACE,
        }
    }
}

// ---- Config (YAML) ----

#[derive(Debug, Deserialize, Clone)]
pub struct Engine {
    pub port: u16,
}

impl Default for Engine {
    fn default() -> Self {
        Self { port: 8080 }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub max_level: LogLevel,
    #[serde(default)]
    pub engine: Engine,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_level: LogLevel::default(),
            engine: Engine::default(),
        }
    }
}

// ---- CliConfig (CLI overrides, mirrors Config shape) ----

#[derive(Parser, Debug)]
#[command(name = "pixors-engine", about = "Pixors image processing engine")]
pub struct CliConfig {
    #[arg(short = 'c', long = "config-file")]
    pub config_file: Option<String>,

    #[arg(short = 'v', long = "verbose")]
    pub verbose: bool,

    #[command(flatten)]
    pub engine: CliEngine,
}

#[derive(Parser, Debug)]
pub struct CliEngine {
    #[arg(short = 'p', long = "port")]
    pub port: Option<u16>,
}

// ---- Layered load: defaults ← YAML ← CLI ----

pub fn load_from(cli: CliConfig) -> Config {
    let path = cli.config_file.as_deref().unwrap_or("config.yaml");

    let mut builder = config::Config::builder()
        .add_source(config::File::from(Path::new(path)).required(false));

    if cli.verbose {
        builder = builder.set_override("max_level", "debug").unwrap();
    }

    if let Some(port) = cli.engine.port {
        builder = builder.set_override("engine.port", port as u64).unwrap();
    }

    builder
        .build()
        .ok()
        .and_then(|c| c.try_deserialize().ok())
        .unwrap_or_default()
}
