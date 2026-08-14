use reqwest::blocking::Client;
use std::{
    env,
    fs,
    io::Write,
    path::PathBuf,
    process::Command,
    thread,
    time::Duration,
};

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> =
        env::args().collect();

    args.windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].clone())
}

fn download(
    url: &str,
    destination: &PathBuf,
) -> Result<(), String> {
    let client = Client::builder()
        .user_agent("MinecraftLauncherUpdater")
        .build()
        .map_err(|e| e.to_string())?;

    let response = client
        .get(url)
        .send()
        .map_err(|e| {
            format!("Download error: {e}")
        })?;

    if !response.status().is_success() {
        return Err(format!(
            "HTTP {}",
            response.status()
        ));
    }

    let data = response
        .bytes()
        .map_err(|e| e.to_string())?;

    let temp = destination.with_extension("update");

    let mut file =
        fs::File::create(&temp)
            .map_err(|e| e.to_string())?;

    file.write_all(&data)
        .map_err(|e| e.to_string())?;

    fs::rename(&temp, destination)
        .map_err(|e| {
            format!(
                "Не удалось заменить launcher: {}",
                e
            )
        })?;

    Ok(())
}

fn main() {
    let url = match argument("--url") {
        Some(value) => value,
        None => {
            eprintln!("Missing --url");
            return;
        }
    };

    let target = match argument("--target") {
        Some(value) => PathBuf::from(value),
        None => {
            eprintln!("Missing --target");
            return;
        }
    };

    /*
     * launcher.exe уже должен завершиться,
     * прежде чем updater попытается заменить его.
     *
     * main launcher после запуска updater
     * должен сделать exit(0).
     */

    thread::sleep(Duration::from_secs(1));

    /*
     * На Windows файл может быть ещё занят.
     * Поэтому несколько попыток.
     */

    let mut success = false;

    for attempt in 0..10 {
        match download(&url, &target) {
            Ok(_) => {
                success = true;
                break;
            }

            Err(error) => {
                eprintln!(
                    "Update attempt {} failed: {}",
                    attempt + 1,
                    error
                );

                thread::sleep(
                    Duration::from_millis(500)
                );
            }
        }
    }

    if !success {
        eprintln!(
            "Launcher update failed"
        );
        return;
    }

    /*
     * Запускаем новую версию.
     */

    let _ = Command::new(&target)
        .spawn();
}