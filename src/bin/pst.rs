use clap::Parser;
use cpst::paste;

#[derive(Parser)]
#[command(name = "pst", version, about = "Paste the system clipboard to stdout")]
struct Pst;

fn main() {
    Pst::parse();

    if let Err(error) = paste() {
        eprintln!("pst: {error}");
        std::process::exit(1);
    }
}
