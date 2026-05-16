#![forbid(unsafe_code)]

fn main() {
    if let Err(error) = spotter::cli::run() {
        eprintln!("{error:#}");
        std::process::exit(1);
    }
}
