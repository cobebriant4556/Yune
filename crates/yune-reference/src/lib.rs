use std::{
    env,
    fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

const SCREENSHOT_MARKER: &str = "YUNE_SCREENSHOT_BASE64:";

pub struct StudioRunResult {
    pub status: ExitStatus,
    pub output_file: PathBuf,
    pub output: String,
}

pub fn locate_studio() -> Result<PathBuf> {
    if let Some(path) = env::var_os("YUNE_STUDIO_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        bail!("YUNE_STUDIO_PATH does not point to a file: {}", path.display());
    }

    #[cfg(target_os = "windows")]
    {
        let local_app_data =
            env::var_os("LOCALAPPDATA").context("LOCALAPPDATA is not set")?;
        let versions = PathBuf::from(local_app_data).join("Roblox").join("Versions");
        let mut candidates = fs::read_dir(&versions)
            .with_context(|| format!("failed to read {}", versions.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path().join("RobloxStudioBeta.exe"))
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        candidates.sort_by_key(|path| {
            fs::metadata(path)
                .and_then(|metadata| metadata.modified())
                .unwrap_or(UNIX_EPOCH)
        });
        return candidates
            .pop()
            .context("Roblox Studio was not found under LOCALAPPDATA");
    }

    #[cfg(target_os = "macos")]
    {
        let path =
            PathBuf::from("/Applications/RobloxStudio.app/Contents/MacOS/RobloxStudio");
        if path.is_file() {
            return Ok(path);
        }
        bail!("Roblox Studio was not found in /Applications");
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        bail!("the official Studio reference backend is supported on Windows and macOS");
    }
}

pub fn run_script(
    script: impl AsRef<Path>,
    place: Option<impl AsRef<Path>>,
    output_file: Option<impl AsRef<Path>>,
) -> Result<StudioRunResult> {
    let studio = locate_studio()?;
    let script = absolute(script.as_ref())?;
    let output_file = match output_file {
        Some(path) => absolute_for_output(path.as_ref())?,
        None => temporary_path("studio-output", "log"),
    };

    let mut command = Command::new(studio);
    command
        .arg("--task")
        .arg("RunScript")
        .arg("--runScriptFile")
        .arg(&script)
        .arg("--outputFile")
        .arg(&output_file)
        .arg("--quitAfterExecution");

    if let Some(place) = place {
        command
            .arg("--localPlaceFile")
            .arg(absolute(place.as_ref())?);
    }

    let status = command.status().context("failed to launch Roblox Studio")?;
    let output = fs::read_to_string(&output_file).unwrap_or_default();

    if !status.success() {
        bail!(
            "Studio RunScript failed with {status}; output file: {}\n{}",
            output_file.display(),
            output
        );
    }

    Ok(StudioRunResult {
        status,
        output_file,
        output,
    })
}

pub fn capture_place(
    place: impl AsRef<Path>,
    png_output: impl AsRef<Path>,
    include_ui: bool,
) -> Result<PathBuf> {
    let script_path = temporary_path("studio-capture", "luau");
    let log_path = temporary_path("studio-capture", "log");
    let png_output = absolute_for_output(png_output.as_ref())?;

    let ui_mode = if include_ui { "All" } else { "None" };
    let script = format!(
        r#"local captureService = game:GetService("StudioCaptureService")
local encodingService = game:GetService("EncodingService")

if not captureService:CanCaptureScreenshot() then
    local granted = captureService:RequestScreenshotPermissionAsync()
    if not granted then
        error("Studio screenshot permission was not granted")
    end
end

local capture = captureService:CaptureScreenshot({{
    Format = Enum.StudioCaptureScreenshotFormat.PNG,
    UICaptureMode = Enum.UICaptureMode.{ui_mode},
}})

while capture.BufferStatus ~= Enum.StudioCaptureBufferStatus.Ready and capture.BufferStatus ~= Enum.StudioCaptureBufferStatus.Error do
    task.wait()
end

if capture.BufferStatus == Enum.StudioCaptureBufferStatus.Error then
    error(table.concat(capture:GetErrors(), "\n"))
end

local encoded = encodingService:Base64Encode(capture:GetBuffer())
print("{SCREENSHOT_MARKER}" .. buffer.tostring(encoded))
"#
    );

    fs::write(&script_path, script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;

    let result = run_script(&script_path, Some(place.as_ref()), Some(&log_path));
    let _ = fs::remove_file(&script_path);
    let result = result?;

    let marker_index = result
        .output
        .find(SCREENSHOT_MARKER)
        .context("Studio output did not contain a screenshot payload")?;
    let encoded = &result.output[marker_index + SCREENSHOT_MARKER.len()..];
    let encoded = encoded
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    let png = STANDARD
        .decode(encoded)
        .context("failed to decode Studio screenshot payload")?;

    if let Some(parent) = png_output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&png_output, png)
        .with_context(|| format!("failed to write {}", png_output.display()))?;

    let _ = fs::remove_file(result.output_file);
    Ok(png_output)
}

fn absolute(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("path does not exist: {}", path.display()))
}

fn absolute_for_output(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    Ok(env::current_dir()?.join(path))
}

fn temporary_path(prefix: &str, extension: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    env::temp_dir().join(format!(
        "yune-{prefix}-{}-{stamp}.{extension}",
        std::process::id()
    ))
}
