//! `capturemann`: the command-line client of the CaptureMann daemon.

use std::process::ExitCode;

fn main() -> ExitCode {
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: cannot start the async runtime: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut env = cm_cli::Env::process();
    let code = runtime.block_on(cm_cli::run(std::env::args(), &mut env));
    ExitCode::from(u8::try_from(code).unwrap_or(1))
}
