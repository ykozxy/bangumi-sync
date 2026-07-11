fn main() {
    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let code = sync_cli::run(std::env::args(), &mut stdout, &mut stderr);
    std::process::exit(code);
}
