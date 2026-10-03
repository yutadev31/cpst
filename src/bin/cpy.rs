use clap::Parser;
use cpst::{Options, Selection, copy, run_daemon};

#[derive(Parser)]
#[command(name = "cpy", version, about = "Copy stdin to the system clipboard")]
struct Cpy {
    #[command(flatten)]
    options: Options,
}

fn main() {
    let mut arguments = std::env::args();
    arguments.next();
    if arguments.next().as_deref() == Some("__internal_daemonize") {
        let selection = if arguments.any(|argument| argument == "--primary") {
            Selection::Primary
        } else {
            Selection::Clipboard
        };
        if let Err(error) = run_daemon(selection) {
            eprintln!("cpy: {error}");
            std::process::exit(1);
        }
        return;
    }

    let cli = Cpy::parse();
    if let Err(error) = copy(cli.options.selection()) {
        eprintln!("cpy: {error}");
        std::process::exit(1);
    }
}
