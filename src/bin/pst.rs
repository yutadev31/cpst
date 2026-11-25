use clap::Parser;
use cpst::{Format, paste_with_format};

#[derive(Parser)]
#[command(name = "pst", version, about = "Paste the system clipboard to stdout")]
struct Pst {
    #[arg(short, long, value_enum, default_value = "auto")]
    format: Format,
}

fn main() {
    let args = Pst::parse();

    if let Err(error) = paste_with_format(args.format) {
        eprintln!("pst: {error}");
        std::process::exit(1);
    }
}
