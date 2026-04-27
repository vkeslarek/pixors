use clap::Parser;

#[derive(Debug, Parser, Clone)]
#[command(name = "pixors-engine", about = "Pixors image processing engine")]
pub struct Config {
    #[arg(short, long, default_value_t = 8399)]
    pub port: u16,
}

impl Default for Config {
    fn default() -> Self {
        Self { port: 8399 }
    }
}
