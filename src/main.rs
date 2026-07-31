fn main() {
    if let Err(error) = aru::run() {
        if !matches!(error, aru::AruError::Reported) {
            eprintln!("error: {error}");
        }
        std::process::exit(1);
    }
}
