pub mod cli;
pub mod core;

pub fn run() -> i32 {
    match cli::run() {
        Ok(code) => code,
        Err(err) => {
            eprintln!("error: {err}");
            1
        }
    }
}
