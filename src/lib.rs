pub mod add;
pub mod apply;
pub mod backup;
pub mod cli;
pub mod config;
pub mod diff;
pub mod doctor;
pub mod edit;
pub mod error;
pub mod path;
pub mod plan;
pub mod state;

pub fn run() -> i32 {
    match cli::run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    }
}
