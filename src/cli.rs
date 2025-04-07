use clap::Parser;

#[derive(Parser)] // requires `derive` feature
#[command(version, about, long_about = None)]
pub struct Cli {
    #[arg(short = 'f')]
    pub file: String,
    #[arg(short = 'p', default_value_t = 8080)]
    pub port: i32,
    #[arg(short = 'u', default_value_t = String::from("http://localhost:8090"))]
    pub upstream: String,
    #[arg(short = 'r', default_value_t = false)]
    pub reload: bool,
}