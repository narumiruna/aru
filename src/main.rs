fn main() {
    if let Err(error) = aru::run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
