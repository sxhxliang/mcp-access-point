use clap::Parser;

#[derive(Parser)] // requires `derive` feature
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'f')]
    pub file: Option<String>,
    #[arg(short = 'c')]
    pub config: Option<String>,
    #[arg(short = 'p', default_value_t = 8080)]
    pub port: i32,
    #[arg(short = 'u')]
    pub upstream: Option<String>,
    #[arg(short = 'r', default_value_t = false)]
    pub reload: bool,
}