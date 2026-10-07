use std::{env, path::PathBuf, process::ExitCode};

use anyhow::{Context, Result, bail};
use lune::Runtime;

fn main() -> ExitCode {
    match async_io::block_on(run()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Yune error: {err:#}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<ExitCode> {
    let mut args = env::args_os().skip(1).collect::<Vec<_>>();
    if args.first().and_then(|v| v.to_str()) == Some("run") {
        args.remove(0);
    }
    if args.is_empty() {
        bail!("usage: yune [run] <script.luau> [args...]");
    }

    let script = PathBuf::from(args.remove(0));
    let script_args = args;
    let state = yune_render::RenderState::default();

    let mut runtime = Runtime::new()
        .context("failed to initialize Lune runtime")?
        .with_args(script_args)
        .with_lib("@yune/render", {
            let state = state.clone();
            move |lua| yune_render::install(lua, state)
        })?;

    let result = runtime
        .run_file(script)
        .await
        .context("failed to run Yune script")?;

    Ok(ExitCode::from(result.status()))
}
