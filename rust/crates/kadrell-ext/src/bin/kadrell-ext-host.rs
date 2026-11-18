//! Eigenständiger Einstieg für den Extension-Helper: `kadrell-ext-host [ext-host] <ordner>`. Die App ruft den Helper
//! über `kadrell ext-host <ordner>`; diese Binärdatei dient Tests und dem Ausprobieren einer Extension von Hand.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "ext-host") {
        args.remove(0);
    }
    let Some(dir) = args.first() else {
        eprintln!("Aufruf: kadrell-ext-host [ext-host] <ordner>");
        return ExitCode::FAILURE;
    };
    kadrell_ext::run_host(&PathBuf::from(dir))
}
