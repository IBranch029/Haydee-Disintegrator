// The release build is a windowed application: no console is attached unless
// the user explicitly asks for the command line interface.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    let action = disintegrator_lib::cli::parse(&arguments);
    if !matches!(action, disintegrator_lib::cli::CliAction::Gui) {
        // The CLI keeps the console attached to the parent shell.
        disintegrator_lib::cli::attach_console();
        let code = disintegrator_lib::cli::execute(action);
        std::process::exit(code);
    }

    if let Err(error) = disintegrator_lib::run() {
        eprintln!("[disintegrator] fatal error: {error}");
        std::process::exit(1);
    }
}
