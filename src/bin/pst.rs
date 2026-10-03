use clap::Parser;
use cpst::{Options, paste};

#[derive(Parser)]
#[command(name = "pst", version, about = "Paste the system clipboard to stdout")]
struct Pst {
    #[command(flatten)]
    options: Options,
}

fn main() {
    let cli = Pst::parse();
    if let Err(error) = paste(cli.options.selection()) {
        eprintln!("pst: {error}");
        std::process::exit(1);
    }
}
