#!/usr/bin/env bash
#
# Instalador de DeepSeek Harness Desktop
#
# Autor: Victor Manuel Agudelo <vicmandev@gmail.com>
# Licencia: MIT
#
# Compila y registra la aplicación de escritorio de DeepSeek Harness:
#   - comprueba los requisitos (Rust, WebKitGTK, Node moderno, dsh)
#   - compila el binario en modo release
#   - instala iconos y lanzador en el menú de aplicaciones
#   - opcionalmente genera un paquete .deb
#
# Uso:
#   ./instalar.sh              instala en el sistema del usuario
#   ./instalar.sh --deb        además genera el paquete .deb
#   ./instalar.sh --desinstalar
#
set -euo pipefail

AUTOR="Victor Manuel Agudelo <vicmandev@gmail.com>"
PROYECTO="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SRC_TAURI="$PROYECTO/src-tauri"
BIN="$SRC_TAURI/target/release/dsh-desktop"

# Rutas de instalación (todo en el HOME, sin necesidad de root).
DIR_APPS="$HOME/.local/share/applications"
DIR_ICONOS="$HOME/.local/share/icons"

GENERAR_DEB=0
DESINSTALAR=0

for arg in "$@"; do
  case "$arg" in
    --deb) GENERAR_DEB=1 ;;
    --desinstalar|--uninstall) DESINSTALAR=1 ;;
    -h|--help) sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Opción desconocida: $arg" >&2; exit 2 ;;
  esac
done

# ---------- utilidades de salida ----------
azul()  { printf '\033[1;34m%s\033[0m\n' "$*"; }
verde() { printf '\033[1;32m%s\033[0m\n' "$*"; }
rojo()  { printf '\033[1;31m%s\033[0m\n' "$*" >&2; }
aviso() { printf '\033[1;33m%s\033[0m\n' "$*"; }
ok()    { printf '  \033[32m✓\033[0m %s\n' "$*"; }
fallo() { printf '  \033[31m✗\033[0m %s\n' "$*"; }

# ---------- desinstalación ----------
if [ "$DESINSTALAR" -eq 1 ]; then
  azul "Desinstalando DeepSeek Harness Desktop…"
  rm -f  "$DIR_APPS/dsh-desktop.desktop"
  rm -rf "$DIR_ICONOS/dsh"
  for t in 32 48 64 128 256 512; do
    rm -f "$DIR_ICONOS/hicolor/${t}x${t}/apps/dsh.png"
  done
  command -v update-desktop-database >/dev/null && \
    update-desktop-database "$DIR_APPS" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null && \
    gtk-update-icon-cache -f -t "$DIR_ICONOS/hicolor" 2>/dev/null || true
  verde "Lanzador e iconos eliminados."
  echo "El código fuente sigue en: $PROYECTO"
  exit 0
fi

azul "════════════════════════════════════════════════"
azul "  DeepSeek Harness Desktop — instalación"
azul "  $AUTOR"
azul "════════════════════════════════════════════════"
echo

# ---------- 1) requisitos ----------
azul "1/6  Comprobando requisitos"

faltan=0

if command -v cargo >/dev/null 2>&1; then
  ok "Rust: $(cargo --version | cut -d' ' -f2)"
else
  fallo "Falta Rust (instala desde https://rustup.rs)"
  faltan=1
fi

# WebKitGTK 4.1 es la dependencia gráfica de Tauri en Linux.
if pkg-config --exists webkit2gtk-4.1 2>/dev/null; then
  ok "WebKitGTK: $(pkg-config --modversion webkit2gtk-4.1)"
else
  fallo "Falta libwebkit2gtk-4.1-dev"
  echo "     sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev"
  faltan=1
fi

for p in gtk+-3.0 libsoup-3.0 javascriptcoregtk-4.1; do
  if pkg-config --exists "$p" 2>/dev/null; then
    ok "$p"
  else
    fallo "Falta $p"
    faltan=1
  fi
done

# Node >= 22 (DSH usa node:util.parseEnv).
# Se ordenan los candidatos por version, de mayor a menor, para quedarnos con
# la mas reciente (el glob de nvm va alfabetico y elegiria v22 antes que v24).
NODO=""
for cand in $(ls -1 "$HOME"/.nvm/versions/node/*/bin/node 2>/dev/null \
              /usr/local/bin/node /usr/bin/node 2>/dev/null | \
              awk -F/ '{v=""; for(i=1;i<=NF;i++) if($i ~ /^v[0-9]+\./) v=$i;
                        print (v=="" ? "v0" : v), $0}' | \
              sort -V -k1,1r | awk '{print $2}'); do
  [ -x "$cand" ] || continue
  v=$("$cand" --version 2>/dev/null | tr -d 'v' | cut -d. -f1)
  if [ -n "$v" ] && [ "$v" -ge 22 ] 2>/dev/null; then
    NODO="$cand"; break
  fi
done
if [ -n "$NODO" ]; then
  ok "Node: $("$NODO" --version)  ($NODO)"
else
  aviso "No se encontró Node >= 22. DSH no arrancará."
  echo "     Instálalo con: nvm install 24"
fi

# El backend dsh debe existir.
DSH_BIN=""
for c in "$HOME/.local/share/pnpm/bin/dsh" "$HOME/.local/bin/dsh" \
         "$(command -v dsh 2>/dev/null || true)"; do
  [ -n "$c" ] && [ -x "$c" ] && { DSH_BIN="$c"; break; }
done
if [ -n "$DSH_BIN" ]; then
  ok "dsh: $DSH_BIN"
else
  fallo "No se encontró el comando dsh"
  echo "     Instálalo con: pnpm add -g @deepseek-ai/dsh"
  faltan=1
fi

if [ "$faltan" -eq 1 ]; then
  echo
  rojo "Faltan requisitos. Resuélvelos y vuelve a ejecutar."
  exit 1
fi
echo

# ---------- 2) compilación ----------
azul "2/6  Compilando la aplicación (puede tardar unos minutos)"
cd "$SRC_TAURI"
if cargo build --release 2>&1 | tail -3; then
  [ -f "$BIN" ] || { rojo "La compilación no produjo el binario"; exit 1; }
  ok "Binario: $BIN ($(du -h "$BIN" | cut -f1))"
else
  rojo "Falló la compilación"
  exit 1
fi
echo

# ---------- 3) iconos ----------
azul "3/6  Instalando iconos"
# Se prefiere el PNG ya compuesto en alta resolucion (1024x1024, generado
# desde el SVG vectorial). El logo suelto se usaria solo como respaldo.
LOGO="$PROYECTO/ui/icono-app.png"
[ -f "$LOGO" ] || LOGO="$PROYECTO/ui/logo-oficial.png"
[ -f "$LOGO" ] || LOGO="$SRC_TAURI/icons/icon.png"

if [ -f "$LOGO" ]; then
  # El icono de la barra de tareas debe tener el exterior transparente: un
  # cuadrado blanco opaco se ve como un recuadro (los temas WhiteSur, Papirus
  # o Adwaita esperan transparencia). Si el PNG ya la trae, se usa tal cual.
  ICONO_BASE=/tmp/dsh-icono-base.png
  TIENE_ALPHA=no
  if command -v identify >/dev/null 2>&1; then
    case "$(identify -format '%A' "$LOGO" 2>/dev/null)" in
      True|Blend) TIENE_ALPHA=si ;;
    esac
  fi

  if [ "$TIENE_ALPHA" = si ]; then
    ICONO_BASE="$LOGO"
  elif command -v convert >/dev/null 2>&1; then
    # El logo venía opaco: le damos esquinas redondeadas y exterior transparente.
    LADO=$(identify -format '%w' "$LOGO" 2>/dev/null || echo 176)
    RADIO=$(( LADO * 22 / 100 ))
    convert -size "${LADO}x${LADO}" xc:none -fill white \
      -draw "roundrectangle 0,0 $((LADO-1)),$((LADO-1)) ${RADIO},${RADIO}" \
      /tmp/dsh-mask.png 2>/dev/null
    convert "$LOGO" /tmp/dsh-mask.png -alpha off \
      -compose CopyOpacity -composite "$ICONO_BASE" 2>/dev/null
    [ -f "$ICONO_BASE" ] || ICONO_BASE="$LOGO"
  else
    ICONO_BASE="$LOGO"
  fi

  for t in 32 48 64 128 256 512; do
    mkdir -p "$DIR_ICONOS/hicolor/${t}x${t}/apps"
    if command -v convert >/dev/null 2>&1; then
      convert "$ICONO_BASE" -resize "${t}x${t}" \
        "$DIR_ICONOS/hicolor/${t}x${t}/apps/dsh.png" 2>/dev/null || \
        cp "$ICONO_BASE" "$DIR_ICONOS/hicolor/${t}x${t}/apps/dsh.png"
    else
      cp "$ICONO_BASE" "$DIR_ICONOS/hicolor/${t}x${t}/apps/dsh.png"
    fi
  done
  mkdir -p "$DIR_ICONOS/dsh"
  cp "$ICONO_BASE" "$DIR_ICONOS/dsh/dsh.png"
  if identify -format '%A' "$DIR_ICONOS/512x512/apps/dsh.png" 2>/dev/null | grep -q True; then
    ok "Iconos con transparencia en $DIR_ICONOS/hicolor/*/apps/dsh.png"
  else
    aviso "Iconos instalados (sin transparencia: falta ImageMagick)"
  fi
else
  aviso "No se encontró el logo; se usará el icono por defecto"
fi
echo

# ---------- 4) lanzador ----------
azul "4/6  Registrando el lanzador en el menú"
mkdir -p "$DIR_APPS"
cat > "$DIR_APPS/dsh-desktop.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Version=1.0
Name=DeepSeek Harness
GenericName=AI Coding Agent
Comment=Agente de codificación con IA de DeepSeek
Comment[en]=DeepSeek AI coding agent
Exec=$BIN
Icon=dsh
Terminal=false
Categories=Development;
Keywords=ai;agent;deepseek;harness;code;coding;
StartupNotify=true
StartupWMClass=dsh-desktop
DESKTOP
chmod +x "$DIR_APPS/dsh-desktop.desktop"
command -v update-desktop-database >/dev/null && \
  update-desktop-database "$DIR_APPS" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && \
  gtk-update-icon-cache -f -t "$DIR_ICONOS/hicolor" 2>/dev/null || true
ok "Lanzador: $DIR_APPS/dsh-desktop.desktop"
echo

# ---------- 5) .deb opcional ----------
azul "5/6  Paquete .deb"
if [ "$GENERAR_DEB" -eq 1 ]; then
  if ! command -v cargo-tauri >/dev/null 2>&1 && \
     ! cargo tauri --version >/dev/null 2>&1; then
    aviso "Falta el CLI de Tauri; instalándolo…"
    cargo install tauri-cli --version "^2" --locked
  fi
  echo "  Generando el paquete…"
  if cargo tauri build --bundles deb 2>&1 | tail -5; then
    DEB=$(find "$SRC_TAURI/target/release/bundle/deb" -name '*.deb' 2>/dev/null | head -1)
    [ -n "$DEB" ] && ok "Paquete: $DEB"
  else
    aviso "No se pudo generar el .deb (la instalación local ya está hecha)"
  fi
else
  echo "  Omitido (usa --deb para generarlo)"
fi
echo

# ---------- 6) resumen ----------
azul "6/6  Listo"
echo
verde "DeepSeek Harness Desktop instalado."
echo
echo "  Abrir desde el menú:  DeepSeek Harness"
echo "  O desde la terminal:  $BIN"
echo
echo "  Desinstalar:          $0 --desinstalar"
echo "  Generar .deb:         $0 --deb"
echo
aviso "Nota: la app usa el puerto 3081 para su servidor local."
aviso "Si está ocupado, buscará otro automáticamente."
