use clap::Parser;
use cpst::{copy, run_daemon};

#[derive(Parser)]
#[command(name = "cpy", version, about = "Copy stdin to the system clipboard")]
struct Cpy;

fn main() {
    let mut arguments = std::env::args();
    arguments.next();

    if arguments.next().as_deref() == Some(cpst::DAEMON_ARGUMENT) {
        if let Err(error) = run_daemon() {
            println!("ERR {}", error.to_string().replace('\n', " "));
            std::process::exit(1);
        }

        return;
    }

    Cpy::parse();

    let result = std::env::current_exe()
        .map_err(|error| error.to_string())
        .and_then(|executable| copy(&executable).map_err(|error| error.to_string()));

    if let Err(error) = result {
        eprintln!("cpy: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_primary_options() {
        assert!(Cpy::try_parse_from(["cpy", "-p"]).is_err());
        assert!(Cpy::try_parse_from(["cpy", "--primary"]).is_err());
    }
}
