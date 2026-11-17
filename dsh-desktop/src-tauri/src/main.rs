// DeepSeek Harness — aplicación de escritorio (Tauri)
//
// Flujo de arranque (no bloqueante):
//   1. Se elige un puerto libre.
//   2. La ventana se abre DE INMEDIATO con una pantalla de carga (splash).
//   3. Un hilo aparte comprueba Node, instala DSH si falta, arranca
//      `dsh web` y espera a que responda.
//   4. Cuando está listo, la ventana navega a la GUI con su token.
//   5. Al cerrar la ventana se termina el proceso hijo.
//
// Módulos:
//   backend       proceso `dsh web` (puerto, arranque, parada)
//   dsh           localizar, instalar y versionar DSH y Node
//   actualizacion avisos de versión nueva por notificación
//   splash        pantalla de carga y recursos de `ui/`
//   textos        idioma del equipo y textos de `ui/textos.json`
//   versiones     comparación semver

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actualizacion;
mod backend;
mod dsh;
mod splash;
mod textos;
mod versiones;

use std::sync::Mutex;

use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

use actualizacion::vigilar_actualizaciones;
use backend::{arrancar_backend, parar_backend, puerto_libre, Backend};
use dsh::{entrypoint_dsh, instalar_dsh_si_falta, node_suficiente, ruta_dsh, version_node};
use splash::servidor_splash;
use textos::idioma_del_equipo;

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
            // Fondo oscuro del webview: al navegar del splash a la GUI,
            // WebKit pinta este color mientras carga el documento nuevo. Con
            // el blanco por defecto se veia un destello de ~1 s entre dos
            // pantallas oscuras. Es un tono intermedio entre ambas.
            .background_color(tauri::window::Color(21, 22, 25, 255))
            .center()
            .build()?;

            // 2) El backend arranca en un hilo aparte, sin bloquear la UI.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                let error_en_splash = |js: String| {
                    if let Some(w) = handle.get_webview_window("main") {
                        let _ = w.eval(&js);
                    }
                };

                // 2a) Sin un Node suficiente ni se puede instalar DSH ni
                //     arrancarlo: se explica en la pantalla de carga en lugar
                //     de esperar 90 s a un servidor que nunca respondera.
                match version_node() {
                    Some(v) if node_suficiente(&v) => {}
                    Some(v) => {
                        return error_en_splash(format!(
                            "window.dshError && window.dshError('errorNode', {{node: '{v}'}});"
                        ));
                    }
                    None => {
                        return error_en_splash(
                            "window.dshError && window.dshError('errorSinNode');".into(),
                        );
                    }
                }

                // 2b) Si DSH no esta instalado, se instala ahora. Hasta aqui
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

                // 2c) Con DSH presente, avisamos si hay una version mas nueva,
                //     ahora y cada pocas horas. Va en su propio hilo para no
                //     retrasar el arranque.
                let h = handle.clone();
                std::thread::spawn(move || vigilar_actualizaciones(h));

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
                parar_backend(ventana);
            }
        })
        .run(tauri::generate_context!())
        .expect("error al ejecutar DeepSeek Harness");
}
