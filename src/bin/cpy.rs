use clap::Parser;
use cpst::{Format, copy_with_format, run_daemon_with_format};

#[derive(Parser)]
#[command(name = "cpy", version, about = "Copy stdin to the system clipboard")]
struct Cpy {
    #[arg(short, long, value_enum, default_value = "auto")]
    format: Format,
}

fn main() {
    let mut arguments = std::env::args_os();
    let executable = arguments.next();

    if arguments
        .next()
        .is_some_and(|arg| arg == cpst::DAEMON_ARGUMENT)
    {
        let args = Cpy::parse_from(executable.into_iter().chain(arguments));
        if let Err(error) = run_daemon_with_format(args.format) {
            println!("ERR {}", error.to_string().replace('\n', " "));
            std::process::exit(1);
        }

        return;
    }

    let args = Cpy::parse();

    let result = std::env::current_exe()
        .map_err(|error| error.to_string())
        .and_then(|executable| {
            copy_with_format(&executable, args.format).map_err(|error| error.to_string())
        });

    if let Err(error) = result {
        eprintln!("cpy: {error}");
        std::process::exit(1);
    }
}
