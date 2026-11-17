//! Proceso `dsh web`: puerto, arranque, espera y parada.

use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::Manager;

use crate::dsh::{dir_node, entrypoint_dsh, path_ampliado, ruta_dsh};

/// Guarda el proceso hijo para poder terminarlo al cerrar la ventana.
pub struct Backend(pub Mutex<Option<Child>>);

/// Puerto preferido de la app.
///
/// La cookie de sesión de DSH lleva la `authority` (host:puerto) firmada.
/// Si el puerto cambiara en cada arranque, WebKit acumularía cookies de
/// sesiones muertas bajo el mismo host `127.0.0.1` y enviaría la incorrecta.
/// Por eso fijamos el puerto; si estuviera ocupado, buscamos otro libre.
pub const PUERTO_PREFERIDO: u16 = 3081;

/// Devuelve el puerto preferido si está libre, o uno libre cualquiera.
pub fn puerto_libre() -> u16 {
    if TcpListener::bind(("127.0.0.1", PUERTO_PREFERIDO)).is_ok() {
        return PUERTO_PREFERIDO;
    }
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(PUERTO_PREFERIDO)
}

/// Termina el `dsh web` hijo, si lo hay.
pub fn parar_backend<R: tauri::Runtime, M: Manager<R>>(m: &M) {
    if let Some(estado) = m.try_state::<Backend>() {
        if let Ok(mut guard) = estado.0.lock() {
            if let Some(mut c) = guard.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
        }
    }
}

/// Espera a que algo acepte conexiones TCP en el puerto dado.
pub fn esperar_servidor(puerto: u16, limite: Duration) -> bool {
    let inicio = Instant::now();
    let dir = format!("127.0.0.1:{puerto}");
    while inicio.elapsed() < limite {
        if std::net::TcpStream::connect(&dir).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(120));
    }
    false
}

/// Arranca `dsh web`, espera la URL con token y devuelve (hijo, url).
pub fn arrancar_backend(puerto: u16) -> (Option<Child>, Option<String>) {
    let args_app = [
        "web",
        "--no-open",
        "--port",
        &puerto.to_string(),
        "--host",
        "127.0.0.1",
    ];

    let mut env_extra: Vec<(String, String)> = Vec::new();
    let mut cmd = match (dir_node(), entrypoint_dsh()) {
        (Some(nodedir), Some(entry)) => {
            let mut c = Command::new(format!("{nodedir}/node"));
            c.arg(&entry);
            if let Some(base) = std::path::Path::new(&entry)
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                env_extra.push(("NODE_PATH".into(), base.to_string_lossy().to_string()));
            }
            c
        }
        _ => match ruta_dsh() {
            Some(b) => Command::new(b),
            None => Command::new("dsh"),
        },
    };

    let mut hijo = cmd
        .args(args_app)
        .envs(env_extra)
        .env("PATH", path_ampliado())
        .env("DSH_HOME", {
            let h = std::env::var("HOME").unwrap_or_default();
            format!("{h}/.dsh")
        })
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok();

    // La URL se lee en un hilo aparte porque `dsh web` la imprime DESPUÉS
    // de levantar el servidor. Si la leyéramos de forma secuencial nos
    // bloquearíamos y perderíamos el token.
    let stdout = hijo.as_mut().and_then(|c| c.stdout.take());
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    if let Some(mut out) = stdout {
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            let mut reader = BufReader::new(&mut out);
            let mut linea = String::new();
            for _ in 0..20 {
                linea.clear();
                match reader.read_line(&mut linea) {
                    Ok(0) => break,
                    Ok(_) => {
                        if let Some(pos) = linea.find("http://") {
                            let _ = tx.send(linea[pos..].trim().to_string());
                            return;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
    }

    // Esperamos a que el puerto acepte conexiones…
    let listo = esperar_servidor(puerto, Duration::from_secs(90));

    // …y damos un margen corto para recibir la URL con su token.
    let url_con_token = rx.recv_timeout(Duration::from_secs(10)).ok();

    let url = if listo {
        Some(url_con_token.unwrap_or_else(|| format!("http://127.0.0.1:{puerto}")))
    } else {
        None
    };

    (hijo, url)
}
