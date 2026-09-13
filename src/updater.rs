use hex::encode;
use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use std::{
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

// ============================================================
// CONFIG
// ============================================================

const GITHUB_REPOSITORY: &str = "cactus-spike1/LauncherMC";
const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const INSTALLER_SUFFIX: &str = "-installer";

#[cfg(unix)]
const CORE_ASSET_NAME: &str = "LauncherMCcore-linux";
#[cfg(windows)]
const CORE_ASSET_NAME: &str = "LauncherMCcore-windows.exe";

#[cfg(unix)]
const LAUNCHER_ASSET_NAME: &str = "LauncherMC-linux";
#[cfg(windows)]
const LAUNCHER_ASSET_NAME: &str = "LauncherMC-windows.exe";

#[cfg(unix)]
const UPDATER_ASSET_NAME: &str = "LauncherMCupdater-linux";
#[cfg(windows)]
const UPDATER_ASSET_NAME: &str = "LauncherMCupdater-windows.exe";

const CORE_NAME: &str = "core";
const LAUNCHER_NAME: &str = "launcher";

const INSTALL_DIR_NAME: &str = "install";

const CORE_HOST: &str = "127.0.0.1";
const CORE_PORT: u16 = 31337;

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}

#[derive(Debug, Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
}

struct DownloadInfo {
    url: String,
    sha256: String,
}

fn release_version(tag: &str) -> Option<Version> {
    tag.strip_prefix('v')?
        .strip_suffix(INSTALLER_SUFFIX)?
        .parse()
        .ok()
}

async fn latest_installer_release(client: &Client) -> Result<(String, Version), String> {
    let releases_url = format!(
        "https://api.github.com/repos/{}/releases?per_page=100",
        GITHUB_REPOSITORY
    );

    let releases = client
        .get(releases_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub releases request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("GitHub releases returned an error: {}", e))?
        .json::<Vec<GithubRelease>>()
        .await
        .map_err(|e| format!("Invalid GitHub releases response: {}", e))?;

    releases
        .into_iter()
        .filter_map(|release| {
            release_version(&release.tag_name).map(|version| (release.tag_name, version))
        })
        .max_by(|(_, left), (_, right)| left.cmp(right))
        .ok_or_else(|| "No installer releases found on GitHub".to_string())
}

async fn github_asset(
    client: &Client,
    tag: &str,
    asset_name: &str,
) -> Result<DownloadInfo, String> {
    let api_url = format!(
        "https://api.github.com/repos/{}/releases/tags/{}",
        GITHUB_REPOSITORY, tag
    );

    let release = client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| format!("GitHub API request failed: {}", e))?
        .error_for_status()
        .map_err(|e| format!("GitHub API returned an error: {}", e))?
        .json::<GithubRelease>()
        .await
        .map_err(|e| format!("Invalid GitHub release response: {}", e))?;

    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == asset_name)
        .ok_or_else(|| format!("GitHub asset not found: {}", asset_name))?;

    let sha256 = match asset
        .digest
        .as_deref()
        .and_then(|digest| digest.strip_prefix("sha256:"))
    {
        Some(digest) => digest.to_string(),
        None => {
            let checksum_asset_name = format!("{}.sha256", asset_name);
            let checksum_asset = release
                .assets
                .iter()
                .find(|item| item.name == checksum_asset_name)
                .ok_or_else(|| {
                    format!("GitHub SHA-256 asset not found: {}", checksum_asset_name)
                })?;

            client
                .get(&checksum_asset.browser_download_url)
                .send()
                .await
                .map_err(|e| format!("Checksum download failed: {}", e))?
                .error_for_status()
                .map_err(|e| format!("Checksum request returned an error: {}", e))?
                .text()
                .await
                .map_err(|e| format!("Checksum read failed: {}", e))?
                .split_whitespace()
                .next()
                .filter(|digest| digest.len() == 64)
                .ok_or_else(|| format!("Invalid SHA-256 asset: {}", checksum_asset_name))?
                .to_string()
        }
    };

    Ok(DownloadInfo {
        url: asset.browser_download_url.clone(),
        sha256,
    })
}

async fn update_updater_if_needed(
    client: &Client,
    current_exe: &Path,
    skip_update: bool,
) -> Result<bool, String> {
    let (installer_tag, latest_version) = latest_installer_release(client).await?;
    let current_version = Version::parse(CURRENT_VERSION)
        .map_err(|e| format!("Invalid updater version {}: {}", CURRENT_VERSION, e))?;

    if skip_update || latest_version <= current_version {
        eprintln!(
            "Updater: current version {} is up to date with {}",
            current_version, latest_version
        );
        return Ok(false);
    }

    eprintln!(
        "Updater: updating updater {} -> {}",
        current_version, latest_version
    );

    let updater_info = github_asset(client, &installer_tag, UPDATER_ASSET_NAME).await?;
    if current_exe.is_file() {
        let current_sha256 = file_sha256(current_exe)?;
        if current_sha256.eq_ignore_ascii_case(&updater_info.sha256) {
            eprintln!("Updater: updater SHA-256 already matches");
            return Ok(false);
        }
    }

    let temp_path = if cfg!(windows) {
        current_exe.with_file_name("LauncherMCupdater.new.exe")
    } else {
        current_exe.with_file_name("LauncherMCupdater.new")
    };

    download(
        client,
        &updater_info.url,
        &temp_path,
        Some(&updater_info.sha256),
    )
    .await?;

    launch_application_with_args(&temp_path, &["--skip-updater-update"])?;
    eprintln!("Updater: handed installation to {}", temp_path.display());
    Ok(true)
}

// ============================================================
// DOWNLOAD
// ============================================================

async fn download(
    client: &Client,
    url: &str,
    destination: &Path,
    expected_sha256: Option<&str>,
) -> Result<(), String> {
    eprintln!("Updater: downloading {} -> {}", url, destination.display());

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("Download error: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("HTTP {}", response.status()));
    }

    let temp = destination.with_extension("update");

    eprintln!("Updater: writing temporary file {}", temp.display());

    let data = response
        .bytes()
        .await
        .map_err(|e| format!("Download read error: {}", e))?;

    let mut hasher = Sha256::new();
    hasher.update(&data);

    let actual_sha256 = encode(hasher.finalize());

    if let Some(expected) = expected_sha256 {
        let expected = expected.trim().to_lowercase();

        if !expected.is_empty() && actual_sha256 != expected {
            return Err(format!(
                "Checksum mismatch for {}\nExpected: {}\nActual:   {}",
                destination.display(),
                expected,
                actual_sha256
            ));
        }

        if !expected.is_empty() {
            eprintln!("Updater: SHA-256 OK for {}", destination.display());
        }
    }

    let mut file =
        fs::File::create(&temp).map_err(|e| format!("Failed to create temp file: {}", e))?;

    file.write_all(&data)
        .map_err(|e| format!("Write error: {}", e))?;

    file.flush().map_err(|e| format!("Flush error: {}", e))?;

    drop(file);

    replace_file(&temp, destination)?;

    #[cfg(unix)]
    {
        if let Err(e) = fs::set_permissions(destination, fs::Permissions::from_mode(0o755)) {
            eprintln!("Updater: failed to set executable permission: {}", e);
        }
    }

    eprintln!("Updater: installed {}", destination.display());

    Ok(())
}

fn file_sha256(path: &Path) -> Result<String, String> {
    let data = fs::read(path).map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let mut hasher = Sha256::new();
    hasher.update(data);
    Ok(encode(hasher.finalize()))
}

// ============================================================
// REPLACE
// ============================================================

fn replace_file(source: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        if destination.exists() {
            fs::remove_file(destination).map_err(|e| {
                format!("Failed to remove old file {}: {}", destination.display(), e)
            })?;
        }
    }

    fs::rename(source, destination).map_err(|e| {
        format!(
            "Failed to replace {} with {}: {}",
            destination.display(),
            source.display(),
            e
        )
    })?;

    Ok(())
}

// ============================================================
// INSTALL COMPONENT
// ============================================================

async fn install_component(
    client: &Client,
    url: &str,
    install_dir: &Path,
    name: &str,
    expected_sha256: &str,
) -> Result<PathBuf, String> {
    let bin_dir = install_dir.join("bin");

    fs::create_dir_all(&bin_dir).map_err(|e| format!("Failed to create bin directory: {}", e))?;

    #[cfg(windows)]
    let destination = {
        let mut destination = bin_dir.join(name);
        destination.set_extension("exe");
        destination
    };

    #[cfg(unix)]
    let destination = bin_dir.join(name);

    let checksum = if expected_sha256.trim().is_empty() {
        None
    } else {
        Some(expected_sha256)
    };

    if let Some(expected) = checksum {
        if destination.is_file() {
            let actual = file_sha256(&destination)?;
            if actual.eq_ignore_ascii_case(expected.trim()) {
                eprintln!(
                    "Updater: {} is already up to date (SHA-256 matches)",
                    destination.display()
                );
                return Ok(destination);
            }
            eprintln!("Updater: {} has an outdated SHA-256", destination.display());
        }
    }

    download(client, url, &destination, checksum).await?;

    Ok(destination)
}

// ============================================================
// LAUNCH
// ============================================================

fn launch_application(application: &Path) -> Result<(), String> {
    launch_application_with_args(application, &[])
}

fn launch_application_with_args(application: &Path, args: &[&str]) -> Result<(), String> {
    eprintln!("Updater: launching {}", application.display());

    let child = Command::new(application)
        .args(args)
        .spawn()
        .map_err(|e| format!("Failed to start {}: {}", application.display(), e))?;

    eprintln!("Updater: started pid={}", child.id());

    Ok(())
}

// ============================================================
// WAIT FOR CORE
// ============================================================

async fn wait_for_core() -> Result<(), String> {
    let address = format!("{}:{}", CORE_HOST, CORE_PORT);

    eprintln!("Updater: waiting for Core on {}", address);

    for attempt in 1..=30 {
        match tokio::net::TcpStream::connect(&address).await {
            Ok(_) => {
                eprintln!("Updater: Core is ready");
                return Ok(());
            }

            Err(_) => {
                eprintln!("Updater: Core not ready ({}/30)", attempt);

                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }

    Err(format!("Core did not start listening on {}", address))
}

// ============================================================
// INSTALL FLOW
// ============================================================

async fn main_install_flow(skip_updater_update: bool) -> Result<(), String> {
    let install_dir = env::current_dir()
        .map_err(|e| e.to_string())?
        .join(INSTALL_DIR_NAME);

    fs::create_dir_all(&install_dir)
        .map_err(|e| format!("Failed to create install directory: {}", e))?;

    let client = Client::builder()
        .user_agent("MinecraftLauncherUpdater/1.0")
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let current_exe =
        env::current_exe().map_err(|e| format!("Failed to locate current updater: {}", e))?;

    if update_updater_if_needed(&client, &current_exe, skip_updater_update).await? {
        return Ok(());
    }

    let (installer_tag, _) = latest_installer_release(&client).await?;
    let release_tag = installer_tag
        .strip_suffix(INSTALLER_SUFFIX)
        .ok_or_else(|| format!("Invalid installer release tag: {}", installer_tag))?;

    let (core_info, launcher_info) = tokio::join!(
        github_asset(&client, release_tag, CORE_ASSET_NAME),
        github_asset(&client, release_tag, LAUNCHER_ASSET_NAME),
    );

    let core_info = core_info?;
    let launcher_info = launcher_info?;

    // ========================================================
    // PARALLEL DOWNLOAD
    // ========================================================

    eprintln!("Updater: starting parallel installation");

    let core_future = async {
        install_component(
            &client,
            &core_info.url,
            &install_dir,
            CORE_NAME,
            &core_info.sha256,
        )
        .await
        .map(Some)
    };

    let launcher_future = async {
        install_component(
            &client,
            &launcher_info.url,
            &install_dir,
            LAUNCHER_NAME,
            &launcher_info.sha256,
        )
        .await
        .map(Some)
    };

    let (core_result, launcher_result) = tokio::join!(core_future, launcher_future);

    let core = core_result?;
    let launcher = launcher_result?;

    eprintln!("Updater: all downloads completed");

    // ========================================================
    // START CORE
    // ========================================================

    if let Some(core_path) = &core {
        launch_application(core_path)?;

        // Если Launcher зависит от Core.
        wait_for_core().await?;
    }

    // ========================================================
    // START LAUNCHER
    // ========================================================

    if let Some(launcher_path) = &launcher {
        launch_application(launcher_path)?;
    }

    Ok(())
}

// ============================================================
// MAIN
// ============================================================

#[tokio::main]
async fn main() {
    let skip_updater_update = env::args().any(|arg| arg == "--skip-updater-update");

    if let Err(error) = main_install_flow(skip_updater_update).await {
        eprintln!("Updater error: {}", error);
        std::process::exit(1);
    }
}
