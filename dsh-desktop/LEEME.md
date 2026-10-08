# DeepSeek Harness Desktop

Aplicación de escritorio para [DeepSeek Harness](https://www.deepseek.com/harness/):
arranca el servidor local, muestra una pantalla de carga y abre la interfaz en
una ventana propia, **sin navegador**.

Autor: **Victor Manuel Agudelo** &lt;vicmandev@gmail.com&gt;

---

## Opción A — Instalar el paquete `.deb` (recomendado)

```bash
sudo apt install ./dsh-desktop_0.1.0_amd64.deb
```

`apt` resuelve las dependencias solo. Después aparece **DeepSeek Harness** en el
menú de aplicaciones.

Para desinstalarlo:

```bash
sudo apt remove deep-seek-harness
```

## Opción B — Instalar en tu carpeta personal (sin root)

```bash
cd dsh-desktop
./instalar.sh
```

No necesita `sudo`: todo queda en `~/.local`. Para generar además el `.deb`:

```bash
./instalar.sh --deb
```

Para deshacerlo:

```bash
./instalar.sh --desinstalar
```

---

## Requisitos

| Componente | Versión | Cómo instalarlo |
|---|---|---|
| Rust | ≥ 1.77 | `curl https://sh.rustup.rs -sSf \| sh` |
| WebKitGTK | 4.1 | `sudo apt install libwebkit2gtk-4.1-dev` |
| Node.js | **≥ 22** | `nvm install 24` |
| `dsh` | 0.2.0-rc.2 | `pnpm add -g @deepseek-ai/dsh` |

> **Node es importante.** DSH usa `node:util.parseEnv`, que no existe en
> Node 18. La app busca automáticamente la versión más reciente de nvm.

El paquete `.deb` ya declara `libwebkit2gtk-4.1-0` y `libgtk-3-0`, así que `apt`
los instala si faltan. Node y `dsh` sí debes instalarlos tú.

---

## Cómo funciona

```
dsh-desktop
   │
   ├─ 1. Sirve la pantalla de carga en 127.0.0.1:<puerto libre>
   │     (mismo sitio que la GUI — ver nota sobre cookies)
   │
   ├─ 2. Arranca `dsh web --port 3081` como proceso hijo
   │
   ├─ 3. Espera a que responda y lee la URL con token
   │
   ├─ 4. Navega a esa URL; el webview sigue el 303,
   │     guarda la cookie de sesión y muestra la interfaz
   │
   └─ 5. Al cerrar la ventana, termina el proceso hijo
```

### Detalles que importan

**El puerto está fijado en 3081.** La cookie de sesión de DSH lleva firmado el
`host:puerto` (`authority`). Con puerto aleatorio se acumulan cookies de sesiones
muertas bajo `127.0.0.1` y el servidor responde 401. Si el 3081 está ocupado, la
app busca otro libre y funciona igual, pero puede tardar algo más la primera vez.

**El splash se sirve desde `127.0.0.1`, no desde `tauri://`.** La cookie de DSH es
`SameSite=Strict`. Si la pantalla de carga viniera del esquema interno, la
navegación posterior sería entre sitios distintos y el navegador **no enviaría
la cookie**: aparecería una página en blanco con error 401.

**Se desactiva el renderizador DMABUF de WebKitGTK.** En equipos con GPU NVIDIA,
WebKitGTK aborta con `Could not create GBM EGL display: EGL_NOT_INITIALIZED`.
La app define `WEBKIT_DISABLE_DMABUF_RENDERER=1` por su cuenta.

---

## Estructura

```
dsh-desktop/
├── instalar.sh              instalador / desinstalador
├── ui/
│   ├── splash.html          pantalla de carga (editable sin recompilar)
│   └── logo-oficial.png     logotipo, con fondo transparente
├── debian/
│   ├── copyright
│   └── changelog.gz
└── src-tauri/
    ├── src/main.rs          arranque, splash y navegación
    ├── tauri.conf.json      configuración y datos del paquete
    ├── Cargo.toml
    └── icons/
```

### Personalizar la pantalla de carga

`ui/splash.html` se lee **en tiempo de ejecución**, así que puedes cambiar
colores, textos o el logotipo y volver a abrir la app — sin recompilar.

Si compilas el `.deb`, ese archivo se instala en
`/usr/share/dsh-desktop/ui/splash.html`.

---

## Compilar a mano

```bash
cd src-tauri
cargo build --release            # binario
cargo tauri build --bundles deb  # paquete .deb
```

---

## Problemas conocidos

**La ventana se queda en blanco o muestra un 401.**
Suele ser una cookie caducada. Ciérrala y borra el almacén:

```bash
rm -f ~/.local/share/com.vicmandev.dsh.desktop/cookies
```

**No arranca y no pasa del splash.**
Comprueba que `dsh` funcione en tu terminal y que Node sea ≥ 22:

```bash
dsh --version
node --version
```

---

## Licencia

MIT. El logotipo de DeepSeek Harness es marca de DeepSeek y se usa únicamente
para identificar la aplicación.
