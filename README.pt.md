# DeepSeek Harness Desktop

[English](README.md) · [Español](README.es.md) · **Português**

Aplicativo de desktop para o [DeepSeek Harness](https://www.deepseek.com/harness/):
inicia o servidor local, mostra uma tela de carregamento e abre a interface em
uma janela própria — **sem navegador**.

> A DeepSeek não publica cliente para Linux (apenas macOS `.dmg` e Windows
> `.exe`). Este projeto preenche essa lacuna com Tauri + WebKitGTK.

![Tela de carregamento](captura-splash.png)

## Instalação

### Opção A — Pacote `.deb`

Baixe o `.deb` em [Releases](../../releases) e instale:

```bash
sudo apt install ./dsh-desktop_0.1.0_amd64.deb
```

O **DeepSeek Harness** aparecerá no menu de aplicativos. Para desinstalar:

```bash
sudo apt remove deep-seek-harness
```

### Opção B — Instalador local (sem root)

```bash
git clone https://github.com/vicman/deepseek-harness-desktop
cd deepseek-harness-desktop
./instalar.sh
```

Tudo fica em `~/.local`. Também aceita `--deb` (gera o pacote) e
`--desinstalar`.

## Requisitos

| Componente | Versão | Instalação |
|---|---|---|
| Rust | ≥ 1.77 | `curl https://sh.rustup.rs -sSf \| sh` |
| WebKitGTK | 4.1 | `sudo apt install libwebkit2gtk-4.1-dev` |
| Node.js | **≥ 22** | `nvm install 24` |
| `dsh` | 0.2.0-rc.2 | **automático** (veja abaixo) |

> **O Node importa.** O DSH usa `node:util.parseEnv`, que não existe no Node
> 18. O aplicativo procura sozinho a versão mais recente do nvm.

O `.deb` já declara `libwebkit2gtk-4.1-0` e `libgtk-3-0`. Você instala o Node.

### Gerenciamento do DSH

O aplicativo **não inclui o DSH**: ele o resolve a cada inicialização, então
nunca fica preso a uma cópia antiga.

- **Se o DSH não estiver instalado**, o aplicativo o instala na inicialização
  (`pnpm add -g`, com `npm install -g` como alternativa). A tela de
  carregamento mostra *«Instalando o DeepSeek Harness…»* enquanto isso.
- **Se houver uma versão mais nova publicada**, a tela de carregamento avisa
  e mostra o comando para atualizar.

**Ele não se atualiza sozinho.** Instalar software em segundo plano sem pedir
permissão é decisão sua, não do aplicativo: ele apenas informa, e você executa:

```bash
pnpm add -g @deepseek-ai/dsh
```

Para comparar a versão instalada com a publicada:

```bash
dsh --version
npm view @deepseek-ai/dsh version
```

## Idiomas

A interface está em **espanhol, inglês e português**. O idioma é escolhido
automaticamente, nesta ordem:

1. O idioma do sistema (`LANG`, `LC_MESSAGES`, `LC_ALL`) — ex.: `pt_BR.UTF-8`.
2. Os idiomas preferidos do seu navegador.
3. **Espanhol por padrão** se nenhum dos três corresponder.

## Como funciona

```
dsh-desktop
   │
   ├─ 1. Serve a tela de carregamento em 127.0.0.1:<porta livre>
   │
   ├─ 2. Inicia `dsh web --port 3081` como processo filho
   │
   ├─ 3. Espera o servidor responder de verdade (requisição HTTP real)
   │
   ├─ 4. Navega com o token; o webview guarda o cookie
   │
   └─ 5. Ao fechar a janela, encerra o processo filho
```

### Decisões de projeto

**A porta é fixada em 3081.** O cookie de sessão do DSH tem o `host:porta`
(`authority`) assinado. Com porta aleatória, cookies de sessões mortas se
acumulam sob `127.0.0.1` e o servidor responde `401`. Se a 3081 estiver
ocupada, ele procura outra livre.

**A tela de carregamento é servida de `127.0.0.1`, não de `tauri://`.** O
cookie do DSH é `SameSite=Strict`: se o splash viesse do esquema interno, a
navegação seguinte seria entre sites diferentes e o navegador **não enviaria
o cookie** (página em branco com erro `401`).

**O renderizador DMABUF do WebKitGTK é desativado.** Com GPU NVIDIA, o
WebKitGTK aborta com `Could not create GBM EGL display: EGL_NOT_INITIALIZED`.
O aplicativo define `WEBKIT_DISABLE_DMABUF_RENDERER=1` por conta própria.

## Personalização

O `ui/splash.html` é lido **em tempo de execução**, então você pode mudar
cores, textos ou o logotipo e apenas reabrir o aplicativo — sem recompilar.

## Compilar

```bash
cd src-tauri
cargo build --release            # binário
cargo tauri build --bundles deb  # pacote .deb
```

## Problemas conhecidos

**Janela em branco ou erro 401.** Normalmente é um cookie expirado:

```bash
rm -f ~/.local/share/com.vicmandev.dsh.desktop/cookies
```

**Piscada branca breve** entre a tela de carregamento e a interface (~1 s).
É o WebKit pintando a tela enquanto o documento carrega. Ainda não resolvido.

**Trava no splash.** Verifique `dsh --version` e se o Node é ≥ 22.

## Estrutura

```
.
├── instalar.sh              instalador / desinstalador
├── ui/
│   ├── splash.html          tela de carregamento (editável sem recompilar)
│   └── logo-splash.png      logotipo, fundo transparente
├── debian/                  copyright e changelog do pacote
└── src-tauri/
    ├── src/main.rs          inicialização, splash e navegação
    ├── tauri.conf.json
    └── icons/
```

## Licença

MIT. O logotipo do DeepSeek Harness é marca da DeepSeek e é usado apenas para
identificar o aplicativo.

---

Autor: **Victor Manuel Agudelo** &lt;vicmandev@gmail.com&gt;
