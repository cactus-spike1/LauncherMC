use serde_json::json;
use serde_json::Value;

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};

use std::process::Child;

use std::sync::{Arc, Mutex};
use std::thread;

mod dependencies;
mod mc;
mod paths;
use mc::minecraft_run::prepare_and_launch;

const SOCKET_ADDR: &str = "127.0.0.1:31337";

fn write_response(stream: &mut TcpStream, v: &Value) -> std::io::Result<()> {
    let s = serde_json::to_string(v).unwrap();
    stream.write_all(s.as_bytes())?;
    stream.write_all(b"\n")?;
    Ok(())
}

fn handle_client(mut stream: TcpStream, state: Arc<Mutex<Option<Child>>>) {
    let peer = stream.peer_addr().ok();
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut buf = Vec::new();

    loop {
        buf.clear();

        // читаем до \n, но не зависаем навсегда
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => break, // клиент закрыл соединение
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf).trim().to_string();

                if line.is_empty() {
                    continue;
                }

                let v: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        let resp = json!({"status":"error","msg":e.to_string()});
                        let _ = write_response(&mut stream, &resp);
                        continue;
                    }
                };

                let cmd = v.get("cmd").and_then(|c| c.as_str()).unwrap_or("");

                let resp = match cmd {
                    "ping" => json!({"status":"ok","cmd":"pong"}),

                    "status" => {
                        let guard = state.lock().unwrap();
                        if let Some(child) = guard.as_ref() {
                            json!({"status":"ok","running":true,"pid":child.id()})
                        } else {
                            json!({"status":"ok","running":false})
                        }
                    }

                    "launch" => {
                        let username = v
                            .get("args")
                            .and_then(|a| a.get("username"))
                            .and_then(|u| u.as_str())
                            .unwrap_or("")
                            .to_string();

                        if username.is_empty() {
                            json!({"status":"error","msg":"missing username"});
                        }

                        let username_clone = username.clone();

                        thread::spawn(move || match prepare_and_launch(&username_clone) {
                            Ok(pid) => {
                                eprintln!("Minecraft launched, pid={}", pid);
                            }
                            Err(e) => {
                                eprintln!("Minecraft launch error: {}", e);
                            }
                        });

                        json!({
                            "status": "ok",
                            "msg": "launch_started"
                        })
                    }

                    _ => json!({"status":"error","msg":"unknown command"}),
                };

                let _ = write_response(&mut stream, &resp);
            }

            Err(_) => break,
        }
    }

    if let Some(p) = peer {
        eprintln!("Connection closed: {:?}", p);
        state.lock().unwrap().take(); // убиваем процесс, если он был запущен
    }
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind(SOCKET_ADDR)?;
    eprintln!("Core: listening on {}", SOCKET_ADDR);

    let state: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let s = state.clone();
                thread::spawn(move || handle_client(stream, s));
            }
            Err(err) => {
                eprintln!("Core: incoming connection error: {}", err);
            }
        }
    }

    Ok(())
}
