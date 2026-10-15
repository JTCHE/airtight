#![windows_subsystem = "windows"]

fn main() {
    airtight_log::open();
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--probe") {
        std::process::exit(airtight_app::probe::run(&args[1..]));
    }
    #[cfg(windows)]
    if args.first().map(String::as_str) == Some("--quit") {
        std::process::exit(airtight_app::ui::quit_running());
    }
    #[cfg(windows)]
    std::process::exit(airtight_app::ui::run());
    #[cfg(target_os = "linux")]
    std::process::exit(airtight_app::cli::run(&args));
}
