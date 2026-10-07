use std::{env, ffi::OsString, path::PathBuf, process::ExitCode};

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
    let command = args
        .first()
        .and_then(|value| value.to_str())
        .unwrap_or("run")
        .to_string();

    match command.as_str() {
        "studio-run" => run_studio_script(args),
        "studio-capture" => capture_studio(args),
        "run" => {
            args.remove(0);
            run_native(args).await
        }
        _ => run_native(args).await,
    }
}

async fn run_native(mut args: Vec<OsString>) -> Result<ExitCode> {
    if args.is_empty() {
        bail!(
            "usage: yune [run] <script.luau> [args...] | yune studio-run <script.luau> [place.rbxl] | yune studio-capture <place.rbxl> <output.png> [--no-ui]"
        );
    }

    let script = PathBuf::from(args.remove(0));
    let mut runtime = Runtime::new()
        .context("failed to initialize Lune runtime")?
        .with_args(args)
        .with_lib("@yune/runtime", yune_runtime::install)?;

    let result = runtime
        .run_file(script)
        .await
        .context("failed to run Yune script")?;

    Ok(ExitCode::from(result.status()))
}

fn run_studio_script(mut args: Vec<OsString>) -> Result<ExitCode> {
    args.remove(0);
    if args.is_empty() {
        bail!("usage: yune studio-run <script.luau> [place.rbxl]");
    }

    let script = PathBuf::from(args.remove(0));
    let place = args.first().map(PathBuf::from);
    let result = yune_reference::run_script(
        script,
        place.as_ref(),
        Option::<&PathBuf>::None,
    )?;
    print!("{}", result.output);
    Ok(ExitCode::SUCCESS)
}

fn capture_studio(mut args: Vec<OsString>) -> Result<ExitCode> {
    args.remove(0);
    if args.len() < 2 {
        bail!("usage: yune studio-capture <place.rbxl> <output.png> [--no-ui]");
    }

    let place = PathBuf::from(args.remove(0));
    let output = PathBuf::from(args.remove(0));
    let include_ui = !args.iter().any(|arg| arg == "--no-ui");
    let output = yune_reference::capture_place(place, output, include_ui)?;
    println!("{}", output.display());
    Ok(ExitCode::SUCCESS)
}
