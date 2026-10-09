// DeepSeek Harness — aplicación de escritorio (Tauri)
//
// Flujo de arranque (no bloqueante):
//   1. Se elige un puerto libre.
//   2. La ventana se abre DE INMEDIATO con una pantalla de carga (splash).
//   3. Un hilo aparte arranca `dsh web` y espera a que responda.
//   4. Cuando está listo, la ventana navega a la GUI con su token.
//   5. Al cerrar la ventana se termina el proceso hijo.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::Write;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

/// Guarda el proceso hijo para poder terminarlo al cerrar la ventana.
struct Backend(Mutex<Option<Child>>);

/// Puerto preferido de la app.
///
/// La cookie de sesión de DSH lleva la `authority` (host:puerto) firmada.
/// Si el puerto cambiara en cada arranque, WebKit acumularía cookies de
/// sesiones muertas bajo el mismo host `127.0.0.1` y enviaría la incorrecta.
/// Por eso fijamos el puerto; si estuviera ocupado, buscamos otro libre.
const PUERTO_PREFERIDO: u16 = 3081;

/// Devuelve el puerto preferido si está libre, o uno libre cualquiera.
fn puerto_libre() -> u16 {
    if TcpListener::bind(("127.0.0.1", PUERTO_PREFERIDO)).is_ok() {
        return PUERTO_PREFERIDO;
    }
    TcpListener::bind("127.0.0.1:0")
        .and_then(|l| l.local_addr())
        .map(|a| a.port())
        .unwrap_or(PUERTO_PREFERIDO)
}

/// Localiza el binario `dsh` en el PATH del usuario.
///
/// Una app lanzada desde el menú no hereda el PATH del shell interactivo,
/// así que probamos las ubicaciones habituales de pnpm/npm/npx.
fn ruta_dsh() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let candidatos = [
        format!("{home}/.local/share/pnpm/bin/dsh"),
        format!("{home}/.local/bin/dsh"),
        format!("{home}/.bun/bin/dsh"),
        "/usr/local/bin/dsh".to_string(),
        "/usr/bin/dsh".to_string(),
    ];
    for c in candidatos.iter() {
        if std::path::Path::new(c).is_file() {
            return Some(c.clone());
        }
    }

    // Ultimo recurso: confiar en el PATH. Si tampoco aparece, devolvemos
    // None para que quien llame sepa que no hay nada instalado (antes se
    // devolvia "dsh" siempre, y eso impedia detectar la ausencia).
    let salida = Command::new("sh")
        .arg("-c")
        .arg("command -v dsh")
        .env("PATH", path_ampliado())
        .output()
        .ok()?;
    if salida.status.success() {
        let ruta = String::from_utf8_lossy(&salida.stdout).trim().to_string();
        if !ruta.is_empty() {
            return Some(ruta);
        }
    }
    None
}

/// Localiza el directorio bin del Node más moderno instalado por nvm.
///
/// DSH requiere Node ≥ 22 (`node:util.parseEnv`); el Node del sistema
/// puede ser más antiguo y romper el arranque.
fn dir_node() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let raiz = format!("{home}/.nvm/versions/node");
    let mut versiones: Vec<String> = std::fs::read_dir(&raiz)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    versiones.sort_by(|a, b| {
        let parse = |s: &str| -> Vec<u32> {
            s.trim_start_matches('v')
                .split('.')
                .filter_map(|p| p.parse().ok())
                .collect()
        };
        parse(b).cmp(&parse(a))
    });
    for v in versiones {
        let bin = format!("{raiz}/{v}/bin");
        if std::path::Path::new(&format!("{bin}/node")).is_file() {
            return Some(bin);
        }
    }
    None
}

/// Localiza el entrypoint real `@deepseek-ai/dsh/lib/bin.js` en el store de pnpm.
///
/// El store conserva directorios de instalaciones anteriores con la misma
/// version de dsh, y `read_dir` no garantiza orden. Para no arrancar un DSH
/// obsoleto, resolvemos primero la ruta que el shim global tiene escrita:
/// es la unica que coincide con lo que el usuario ejecuta en su terminal.
fn entrypoint_dsh() -> Option<String> {
    let home = std::env::var("HOME").ok()?;

    // 1) La referencia del shim global es la fuente de verdad.
    let shim = format!("{home}/.local/share/pnpm/bin/dsh");
    if let Ok(contenido) = std::fs::read_to_string(&shim) {
        // El shim apunta a global/v11/<hash>; tomamos el hash mas largo.
        let mut mejor: Option<String> = None;
        for parte in contenido.split(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '/') {
            if let Some(resto) = parte.split("global/v11/").nth(1) {
                let hash = resto.split('/').next().unwrap_or("").to_string();
                if hash.len() > 16 {
                    if mejor.as_ref().map_or(true, |m| hash.len() > m.len()) {
                        mejor = Some(hash);
                    }
                }
            }
        }
        if let Some(hash) = mejor {
            let candidato = format!(
                "{home}/.local/share/pnpm/global/v11/{hash}/node_modules/@deepseek-ai/dsh/lib/bin.js"
            );
            if std::path::Path::new(&candidato).is_file() {
                return Some(candidato);
            }
        }
    }

    // 2) Respaldo: el directorio modificado mas recientemente.
    let raiz = format!("{home}/.local/share/pnpm/global/v11");
    let mut candidatos: Vec<(std::time::SystemTime, String)> = Vec::new();
    for entrada in std::fs::read_dir(&raiz).ok()?.filter_map(|e| e.ok()) {
        let ruta = entrada.path().join("node_modules/@deepseek-ai/dsh/lib/bin.js");
        if ruta.is_file() {
            let t = entrada
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            candidatos.push((t, ruta.to_string_lossy().to_string()));
        }
    }
    candidatos.sort_by(|a, b| b.0.cmp(&a.0));
    candidatos.into_iter().next().map(|(_, r)| r)
}

/// PATH ampliado para que el proceso hijo encuentre node y dsh.
fn path_ampliado() -> String {
    let actual = std::env::var("PATH").unwrap_or_default();
    let home = std::env::var("HOME").unwrap_or_default();
    let node = dir_node().unwrap_or_default();
    format!("{node}:{home}/.local/share/pnpm/bin:{home}/.local/bin:{home}/.bun/bin:{actual}")
}

/// Idioma de la interfaz segun el equipo: `es`, `en` o `pt`.
///
/// Se leen las variables de entorno del sistema (las mismas que usa el
/// escritorio). Espanol, ingles y portugues; cualquier otro idioma cae al
/// espanol, que es el idioma por defecto de la aplicacion.
fn idioma_del_equipo() -> &'static str {
    // LC_ALL manda sobre LC_MESSAGES, y este sobre LANG.
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        let valor = std::env::var(var).unwrap_or_default().to_lowercase();
        if valor.is_empty() {
            continue;
        }
        // Formatos: "es_CO.UTF-8", "en_US", "pt_BR.utf8", "es_CO:es"
        let base = valor
            .split(['.', ':', '@'])
            .next()
            .unwrap_or("")
            .split(['_', '-'])
            .next()
            .unwrap_or("")
            .to_string();
        match base.as_str() {
            "es" => return "es",
            "en" => return "en",
            "pt" => return "pt",
            // Cualquier otro idioma: se sigue probando la variable siguiente.
            _ => continue,
        }
    }
    // Sin coincidencia, espanol por defecto.
    "es"
}

/// Version de DSH que hay instalada, leida de su package.json.
fn version_instalada(entrypoint: &str) -> Option<String> {
    // <...>/node_modules/@deepseek-ai/dsh/lib/bin.js
    //   -> subimos a la raiz del paquete y leemos package.json
    let paquete = std::path::Path::new(entrypoint)
        .parent()? // lib/
        .parent()? // raiz del paquete
        .join("package.json");
    let texto = std::fs::read_to_string(paquete).ok()?;
    let json: serde_json::Value = serde_json::from_str(&texto).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
}

/// Version estable mas reciente publicada en npm.
///
/// Se consulta el registro directamente para no depender de que `npm` este en
/// el PATH. Devuelve `None` si no hay red o el registro no responde: en ese
/// caso la aplicacion sigue arrancando con lo que haya instalado.
fn version_publicada() -> Option<String> {
    let dir = "registry.npmjs.org:443";
    let respuesta = https_get(dir, "/@deepseek-ai%2Fdsh/latest")?;
    let json: serde_json::Value = serde_json::from_str(&respuesta).ok()?;
    json.get("version")?.as_str().map(|s| s.to_string())
}

/// Peticion HTTPS GET minima y sin dependencias externas.
///
/// Solo se usa para consultar el registro de npm; si algo falla devuelve
/// `None` y la aplicacion continua sin avisar de actualizaciones.
fn https_get(dir: &str, ruta: &str) -> Option<String> {
    // Resolucion DNS + conexion TLS mediante `curl`, presente en cualquier
    // escritorio Linux. Asi no arrastramos un cliente TLS al binario.
    let salida = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "8",
            "-H",
            "Accept: application/json",
            &format!("https://{dir}{ruta}"),
        ])
        .stdin(Stdio::null())
        .output()
        .ok()?;

    if !salida.status.success() {
        return None;
    }
    String::from_utf8(salida.stdout).ok()
}

/// Instala DSH globalmente si no esta presente.
///
/// Se ejecuta solo cuando `entrypoint_dsh()` y `ruta_dsh()` no encuentran
/// nada: en ese caso la aplicacion no tendria backend con el que arrancar.
/// Se usa pnpm si esta disponible (es el gestor que DSH documenta) y si no,
/// npm. Devuelve `true` si la instalacion termino bien.
fn instalar_dsh_si_falta() -> bool {
    if entrypoint_dsh().is_some() || ruta_dsh().is_some() {
        return true; // ya esta instalado
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let path = path_ampliado();

    // pnpm y npm son scripts de Node, asi que necesitan encontrar `node` en
    // el PATH: se lo pasamos ampliado. Preferimos pnpm (el gestor que DSH
    // documenta) y dejamos npm como respaldo.
    let ordenes = [
        format!("{home}/.local/share/pnpm/bin/pnpm add -g @deepseek-ai/dsh"),
        format!("{home}/.local/share/pnpm/bin/pnpm add -g @deepseek-ai/dsh --force"),
        "npm install -g @deepseek-ai/dsh".to_string(),
    ];

    for orden in ordenes {
        let salida = Command::new("sh")
            .arg("-c")
            .arg(&orden)
            .env("PATH", &path)
            .stdin(Stdio::null())
            .output();

        // Basta con que el entrypoint aparezca: es la comprobacion que
        // realmente importa, no el codigo de salida del gestor.
        if salida.map(|o| o.status.success()).unwrap_or(false) && entrypoint_dsh().is_some() {
            return true;
        }
    }
    false
}

/// Avisa en la pantalla de carga si hay una version mas nueva de DSH.
///
/// No actualiza por su cuenta: instalar software en segundo plano sin pedir
/// permiso es una decision del usuario, no de la aplicacion. Solo informa.
fn avisar_de_actualizacion(ventana: &tauri::WebviewWindow, entrypoint: &str) {
    let instalada = match version_instalada(entrypoint) {
        Some(v) => v,
        None => return,
    };
    let publicada = match version_publicada() {
        Some(v) => v,
        None => return, // sin red: no molestamos
    };
    if publicada == instalada {
        return;
    }

    // Solo avisamos si la publicada es realmente mas nueva (comparacion
    // numerica por componentes, sin tener en cuenta pre-releases).
    if !es_mas_nueva(&publicada, &instalada) {
        return;
    }

    let aviso = format!(
        "window.dshAviso && window.dshAviso('avisoVersion', '{publicada}');"
    );
    let _ = ventana.eval(&aviso);
}

/// Compara versiones tipo `1.2.3` o `0.2.0-rc.2` por componentes numericos.
fn es_mas_nueva(candidata: &str, actual: &str) -> bool {
    let nums = |v: &str| -> Vec<u32> {
        v.split(['.', '-'])
            .take(3)
            .map(|p| p.parse::<u32>().unwrap_or(0))
            .collect()
    };
    let (a, b) = (nums(candidata), nums(actual));
    a > b
}

/// Sirve la pantalla de carga en `127.0.0.1`, en su propio puerto.
///
/// Devuelve el puerto. El HTML queda embebido en el binario para que la
/// ventana pueda nacer ya en `127.0.0.1` (mismo sitio que la GUI) y la
/// cookie `SameSite=Strict` no se bloquee al navegar despues.
fn servidor_splash() -> u16 {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(_) => return 0,
    };
    let puerto = listener.local_addr().map(|a| a.port()).unwrap_or(0);

    std::thread::spawn(move || {
        for flujo in listener.incoming() {
            let mut s = match flujo {
                Ok(s) => s,
                Err(_) => continue,
            };
            // Leemos la peticion para saber que recurso se pide.
            // Solo UNA lectura: si se leyera dos veces, la primera consumiria
            // la peticion y la segunda devolveria 0 bytes, de modo que
            // siempre se serviria el HTML y nunca el logotipo.
            let mut buf = [0u8; 2048];
            let n = std::io::Read::read(&mut s, &mut buf).unwrap_or(0);
            let peticion = String::from_utf8_lossy(&buf[..n]).to_string();

            // El logotipo se sirve como archivo aparte, no incrustado en el
            // HTML: un base64 grande obliga al navegador a decodificarlo
            // antes de pintar y dejaba un hueco visible sin el logo.
            let (tipo, cuerpo): (&str, Vec<u8>) = if peticion.contains("logo-splash.png") {
                ("image/png", cargar_logo())
            } else {
                ("text/html; charset=utf-8", cargar_splash().into_bytes())
            };

            let cabecera = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {tipo}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                cuerpo.len()
            );
            let _ = std::io::Write::write_all(&mut s, cabecera.as_bytes());
            let _ = std::io::Write::write_all(&mut s, &cuerpo);
            let _ = s.flush();
        }
    });

    puerto
}

/// Candidatos donde puede estar un recurso de `ui/` (splash, logotipo...).
///
/// Se cubren tres escenarios: ejecucion desde `target/release`, instalacion
/// en `~/.local/share` y paquete `.deb` en `/usr/share`.
fn rutas_ui(nombre: &str) -> Vec<std::path::PathBuf> {
    let mut rutas: Vec<std::path::PathBuf> = Vec::new();

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            // Desde target/release hay que subir tres niveles hasta la raiz
            // del proyecto; se prueban varias profundidades para tolerar
            // tambien una instalacion junto al binario.
            rutas.push(dir.join(nombre));
            rutas.push(dir.join(format!("ui/{nombre}")));
            rutas.push(dir.join(format!("../ui/{nombre}")));
            rutas.push(dir.join(format!("../../ui/{nombre}")));
            rutas.push(dir.join(format!("../../../ui/{nombre}")));
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    rutas.push(std::path::PathBuf::from(format!(
        "{home}/.local/share/dsh-desktop/ui/{nombre}"
    )));
    rutas.push(std::path::PathBuf::from(format!(
        "/usr/share/dsh-desktop/ui/{nombre}"
    )));
    rutas.push(std::path::PathBuf::from(format!(
        "/usr/local/share/dsh-desktop/ui/{nombre}"
    )));

    rutas
}

/// Carga la pantalla de carga desde `ui/splash.html`.
///
/// Se lee en tiempo de ejecucion (no incrustada) para poder ajustar el diseno
/// sin recompilar el binario de Rust.
fn cargar_splash() -> String {
    for r in rutas_ui("splash.html") {
        if let Ok(t) = std::fs::read_to_string(&r) {
            return t;
        }
    }
    // Ultimo recurso: pagina minima si falta el archivo.
    "<!DOCTYPE html><html><body style=\"margin:0;background:#0d0f12;color:#e6e8eb;\n     display:grid;place-items:center;height:100vh;font:15px system-ui\">\n     <p>Cargando DeepSeek Harness...</p></body></html>".to_string()
}

/// Carga el logotipo de la pantalla de carga como bytes PNG.
///
/// Se sirve como recurso aparte en lugar de incrustarlo en el HTML: un base64
/// grande obliga al navegador a decodificarlo antes de pintar, y ese hueco se
/// veia como un destello claro sin el logo.
fn cargar_logo() -> Vec<u8> {
    for r in rutas_ui("logo-splash.png") {
        if let Ok(b) = std::fs::read(&r) {
            return b;
        }
    }
    Vec::new()
}

/// Espera a que algo acepte conexiones TCP en el puerto dado.
fn esperar_servidor(puerto: u16, limite: Duration) -> bool {
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
fn arrancar_backend(puerto: u16) -> (Option<Child>, Option<String>) {
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

fn main() {
    // WebKitGTK falla al crear el display EGL con el driver propietario de
    // NVIDIA ("Could not create GBM EGL display: EGL_NOT_INITIALIZED").
    // Desactivar el renderizador DMABUF evita ese aborto.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    let puerto = puerto_libre();

    tauri::Builder::default()
        .manage(Backend(Mutex::new(None)))
        .setup(move |app| {
            // 1) Ventana inmediata mostrando la pantalla de carga.
            //
            // La cookie de sesion de DSH es `SameSite=Strict`: si el splash
            // viniera del esquema interno `tauri://localhost`, la navegacion
            // posterior a `127.0.0.1` seria cross-site y el webview no
            // enviaria la cookie (401 + pagina en blanco).
            //
            // Servimos el splash desde un servidor propio en `127.0.0.1`
            // (mismo sitio que la GUI, distinto puerto). Para SameSite el
            // sitio es el host, no el puerto, asi que la cookie viaja.
            let puerto_splash = servidor_splash();
            // El idioma viaja en la URL para que la pantalla de carga lo
            // aplique desde el primer pintado, sin parpadeo de texto.
            let url_splash =
                format!("http://127.0.0.1:{puerto_splash}/?lang={}", idioma_del_equipo());
            let ventana = WebviewWindowBuilder::new(
                app,
                "main",
                WebviewUrl::External(
                    url_splash
                        .parse()
                        .unwrap_or_else(|_| "http://127.0.0.1:3081/".parse().unwrap()),
                ),
            )
            .title("DeepSeek Harness")
            .inner_size(1280.0, 860.0)
            .min_inner_size(900.0, 600.0)
            .center()
            .build()?;

            // 2) El backend arranca en un hilo aparte, sin bloquear la UI.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                // 2a) Si DSH no esta instalado, se instala ahora. Hasta aqui
                //     la ventana ya esta visible con la pantalla de carga, asi
                //     que la espera no se percibe como un cuelgue.
                let esta = entrypoint_dsh().is_some() || ruta_dsh().is_some();
                if !esta {
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.eval(
                            "window.dshEstado && window.dshEstado('instalando');",
                        );
                    }
                    if !instalar_dsh_si_falta() {
                        if let Some(w) = handle.get_webview_window("main") {
                            let _ = w.eval(
                                "window.dshError && window.dshError('errorInstalar');",
                            );
                        }
                        return;
                    }
                }

                // 2b) Con DSH presente, avisamos si hay una version mas nueva.
                //     Va en su propio hilo para no retrasar el arranque.
                if let (Some(w), Some(entry)) =
                    (handle.get_webview_window("main"), entrypoint_dsh())
                {
                    std::thread::spawn(move || avisar_de_actualizacion(&w, &entry));
                }

                let (hijo, url) = arrancar_backend(puerto);

                // Guardamos el hijo para poder matarlo al cerrar.
                if let Some(estado) = handle.try_state::<Backend>() {
                    if let Ok(mut guard) = estado.0.lock() {
                        *guard = hijo;
                    }
                }

                // 3) Navegamos a la GUI, o avisamos del fallo en el splash.
                let win = handle.get_webview_window("main");
                match url {
                    Some(u) => {
                        if let Some(w) = win {
                            // Navegamos DIRECTAMENTE a la URL con token, sin
                            // limpiar cookies antes: la limpieza con set_cookie
                            // reescribia atributos (perdia SameSite/HttpOnly) y
                            // dejaba el almacen en un estado que el servidor
                            // rechazaba. El flujo nativo 303 + Set-Cookie es el
                            // mismo que sigue un navegador normal.
                            if let Ok(parsed) = u.parse() {
                                let _ = w.navigate(parsed);
                            }
                        }
                    }
                    None => {
                        if let Some(w) = win {
                            let _ = w.eval(
                                "window.dshError && window.dshError('errorServidor');",
                            );
                        }
                    }
                }
            });

            let _ = ventana;
            Ok(())
        })
        .on_window_event(|ventana, evento| {
            if let tauri::WindowEvent::Destroyed = evento {
                if let Some(estado) = ventana.try_state::<Backend>() {
                    if let Ok(mut guard) = estado.0.lock() {
                        if let Some(mut c) = guard.take() {
                            let _ = c.kill();
                            let _ = c.wait();
                        }
                    }
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error al ejecutar DeepSeek Harness");
}
