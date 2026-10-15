fn main() {
    std::process::exit(clipsave::run(std::env::args_os().skip(1), false));
}
