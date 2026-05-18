<p align="center">
  <img src="crates/adapter-gui/ui/assets/openrig-logomark.svg" alt="Logomarca de OpenRig" height="120"><img src="crates/adapter-gui/ui/assets/openrig-logotype.png" alt="OpenRig" height="120">
</p>

<p align="center">
  <strong>Construye tu rig una vez. Úsalo en cualquier sitio.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-GPL--3.0-blue.svg" alt="Licencia: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/version-0.1.0--dev-orange.svg" alt="Versión: 0.1.0-dev">
  <img src="https://img.shields.io/badge/rust-2021_edition-brightgreen.svg" alt="Rust: edición 2021">
  <img src="https://img.shields.io/badge/platforms-macOS%20%7C%20Linux%20%7C%20Windows-lightgrey.svg" alt="Plataformas: macOS | Linux | Windows">
  <a href="https://github.com/jpfaria/OpenRig/actions/workflows/test.yml"><img src="https://github.com/jpfaria/OpenRig/actions/workflows/test.yml/badge.svg?branch=develop" alt="Tests"></a>
  <a href="https://codecov.io/gh/jpfaria/OpenRig"><img src="https://codecov.io/gh/jpfaria/OpenRig/branch/develop/graph/badge.svg" alt="Cobertura"></a>
</p>

<p align="center">
  <a href="README.md">English</a> · <a href="README.pt-BR.md">Português</a> · <strong>Español</strong>
</p>

<p align="center">
  <img src="docs/assets/sc1.png" alt="OpenRig — vista de proyecto con varias cadenas paralelas y bloques de amp, pedales y cab" width="900">
</p>

---

> **El audio profesional no debería caber dentro de una caja negra.**

OpenRig es una plataforma de código abierto de procesamiento de audio en tiempo real escrita en Rust. **El software es el producto. El hardware es solo donde corre.**

## Por qué existe OpenRig

Si quieres procesamiento de guitarra a nivel profesional hoy, compras una caja negra. Helix, Quad Cortex, Axe-Fx — miles de dólares por músico. Firmware cerrado. Lo que viene en la caja es lo que tienes, y es lo que nunca te van a dejar cambiar.

Multiplica eso por una banda entera y la cuenta deja de cuadrar.

OpenRig nació de una pregunta simple:

> ¿Y si un único nodo procesara el sonido de toda la banda, y cada músico controlara su propia cadena desde su móvil?

Imagina el escenario. Guitarra, bajo, teclado, voz — todos enchufados a un único nodo. Ese nodo puede ser una pedalera en el suelo, una cajita dentro del gig bag, o un escritorio entre bastidores. **El form factor no importa. El software es el mismo.**

Cada músico abre la app en el móvil o el tablet y controla su propia cadena de efectos. Quien prefiera hardware enchufa una pedalera que es solo un terminal — conectada por USB, Bluetooth o red vía gRPC. Solo una persona de la banda necesita el hardware. El resto usa lo que ya lleva en el bolsillo.

Ese es el destino. Abajo está lo que ya funciona, y lo que viene después.

## Lo que ya funciona hoy

La base que hace posible la visión más grande ya corre en todas las plataformas de escritorio:

- **App standalone** para macOS (Apple Silicon + Intel), Linux (x86_64 + aarch64) y Windows (x86_64).
- **Cadenas verdaderamente paralelas.** Cada input es un runtime de audio aislado — sin búferes compartidos, sin locks contendidos, sin picos de CPU entre streams. ¿Dos guitarras en la misma interfaz? Dos rigs completamente independientes en el mismo proyecto, procesados en paralelo.
- **[560+ modelos registrados](docs/user-guide/blocks-reference.md#model-id-quick-reference)** repartidos en 16 tipos de bloque — preamps, amps, cabs, pedales de overdrive/distorsión/fuzz/boost, delays, reverbs, modulation, dynamics, filtros, wah, corrección de pitch y 114 IRs de cuerpo acústico para pastillas piezo y magnéticas. ([catálogo completo con IDs canónicos](docs/user-guide/blocks-reference.md))
- **Cuatro backends de audio en el mismo grafo.** DSP nativo en Rust para utility, EQ, dynamics, modulation y reverb. NAM (Neural Amp Modeler) con capturas neuronales de hardware real — Marshall Plexi, Mesa Rectifier, EVH 5150, Vox AC30, Klon Centaur, Boss DS-1, Big Muff y 540+ más. Convolución por IR para cabinets y cuerpos acústicos. 100+ plugins LV2 ya incluidos (Guitarix, MDA, TAP, ZAM, Dragonfly y otros). Cualquier bloque en una cadena puede venir de cualquier backend.
- **Visualización en tiempo real integrada.** Un afinador cromático y un analizador de espectro en vivo entran en la cadena como cualquier otro bloque — ve lo que oyes.
- **Controlable por IA (MCP).** Cualquier cliente MCP (Claude Desktop/Code, Cursor) maneja la rig *viva* mediante el servidor MCP integrado de OpenRig — arma timbres, ajusta la cadena, cambia preset por conversación, con la GUI abierta. Ver **[Servidor MCP y plugin](docs/mcp.md)**.
- **Formato de preset YAML abierto.** Los presets son texto plano — diffeables, compartibles por gist, scriptables. La skill [`openrig-tone-builder`](skills/openrig-tone-builder/SKILL.md) de Claude Code arma presets completos a partir del nombre de una canción, investigando la cadena de señal original en fuentes públicas y escribiendo el YAML.

> 📚 **¿Buscas un amp, pedal o cab concreto?** El catálogo completo — cada modelo, cada parámetro, cada variante de voicing, con strings canónicos de `MODEL_ID` para usar en preset YAML — está documentado en **[Blocks Reference](docs/user-guide/blocks-reference.md)**. Empieza por el [Model ID Quick Reference](docs/user-guide/blocks-reference.md#model-id-quick-reference), una búsqueda alfabética agrupada por tipo de bloque.

## Hacia dónde va

El escritorio es la base. El producto es la **banda en un solo nodo**. El camino:

- **Servidor gRPC** — para que clientes externos (móvil, tablet, controlador dedicado, otra instancia de OpenRig) puedan controlar sus propias cadenas por la red en tiempo real.
- **App de móvil y tablet** — la superficie de control por músico. La abres, ves tu cadena, giras knobs.
- **Pedalera como nodo** — hardware tipo Orange Pi corriendo OpenRig con I/O de audio integrada y Linux de baja latencia por debajo.
- **Pedalera como terminal** — el mismo hardware corre como controlador físico de un nodo remoto, hablando USB / Bluetooth / red.
- **Proyectos multi-músico** — un nodo alojando cadenas independientes y aisladas para guitarra, bajo, teclado, voz — cada una controlada desde una superficie distinta.

El mismo software en cualquier form factor. El timbre del usuario va con él — escritorio hoy, gig bag mañana, pedalera en el venue, servidor en el local de ensayo. Nada que reaprender. Nada que re-licenciar.

## Showcase

<p align="center">
  <img src="docs/assets/sc2.png" alt="Biblioteca de bloques — lista vertical de pedales y amps con arte de panel fiel al hardware" width="280">&nbsp;&nbsp;&nbsp;
  <img src="docs/assets/sc3.png" alt="Editor por bloque — panel del Marshall JTM45 con knobs de canal y volumen" width="600">
</p>

Izquierda: biblioteca de bloques, organizada por marca con arte de panel fiel al hardware. Derecha: editor por bloque sobre una captura del Marshall JTM45 — controles exactos, respuesta exacta.

## Inicio rápido

1. **Instala** — [descarga un release](https://github.com/jpfaria/OpenRig/releases/latest) para tu plataforma, o compila desde el código (ver abajo).
2. **Configura I/O** — elige tu interfaz de audio como input y tus monitores/auriculares como output.
3. **Arma una cadena** — arrastra bloques entre Input y Output (Tuner → EQ → Drive → Amp → Cab → Reverb es un buen comienzo).
4. **Ajusta en tiempo real** — haz clic en cualquier bloque para abrir el editor; gira knobs mientras tocas.
5. **Guarda un preset** — los presets son YAML plano en `~/.openrig/presets/` (macOS/Linux) o `%APPDATA%\OpenRig\presets\` (Windows). Compártelos copiando y pegando.

Walkthrough completo: [Quick Start Guide](docs/user-guide/quick-start.md).

## Arma tu timbre

Un preset es solo YAML. Aquí el inicio de una cadena rítmica estilo Frusciante de "Can't Stop":

```yaml
id: red_hot_chili_peppers_-_cant_stop_-_rhythm
name: Red Hot Chili Peppers - Can't Stop (Rhythm)
blocks:
  - type: gain
    enabled: true
    model: cc_boost            # MXR Micro Amp clean boost
    params: {}
  - type: gain
    enabled: true
    model: boss_ds1            # Proxy del Boss DS-2: tone 7, dist 5
    params: { tone: 7, dist: 5 }
  - type: modulation
    enabled: true
    model: ensemble_chorus     # CE-1 Chorus Ensemble
    params: { rate_hz: 0.55, depth: 22.0, mix: 25.0 }
  - type: amp
    enabled: true
    model: marshall_super_100_1966   # Proxy del Marshall Major
    params: {}
  # ...EQ post-amp, reverb, limiter, master volume
```

Cada `model:` ID está registrado en el [Blocks Reference Quick Reference](docs/user-guide/blocks-reference.md#model-id-quick-reference). Para usuarios de Claude Code, la skill [`openrig-tone-builder`](skills/openrig-tone-builder/SKILL.md) genera la cadena entera solo a partir de artista + canción.

## Instalación

### Descarga

Los binarios para todas las plataformas soportadas (macOS aarch64/x86_64, Linux x86_64/aarch64, Windows x86_64) están en la [página de Releases](https://github.com/jpfaria/OpenRig/releases/latest). Cada SO tiene sus pasos en el Installation Guide:

- **macOS** — [descarga, fix de cuarentena e instalador de una línea](docs/user-guide/installation.md#macos)
- **Linux** — [AppImage / `.deb` / `.rpm` + setup de audio](docs/user-guide/installation.md#linux)
- **Windows** — [instalador `.msi` / zip portátil](docs/user-guide/installation.md#windows)

### Compilar desde el código

```bash
git clone https://github.com/jpfaria/OpenRig.git
cd OpenRig
git submodule update --init --recursive
cargo build --release -p adapter-gui
```

Mira el [Installation Guide](docs/user-guide/installation.md) para dependencias por plataforma y troubleshooting.

## Documentación

### Para músicos

- [Installation Guide](docs/user-guide/installation.md) — descargar, compilar, configurar
- [Quick Start](docs/user-guide/quick-start.md) — primer proyecto y signal chain
- [Blocks Reference](docs/user-guide/blocks-reference.md) — cada modelo con IDs canónicos y parámetros
- [Presets](docs/user-guide/presets.md) — crear, guardar, compartir
- [Blocks Catalog](docs/blocks-catalog.md) — tipos de bloque, modelos, parámetros, backends
- [Screens](docs/screens.md) — Launcher, Chains, Tuner, Spectrum, Block Editor
- [Audio Config](docs/audio-config.md) — I/O como bloques, ciclo de vida de JACK
- [CLI & env vars](docs/cli.md) — argumentos y variables de entorno de `openrig`
- [Servidor MCP y plugin](docs/mcp.md) — controlar la rig desde Claude/Cursor; instalar el plugin OpenRig
- [Controlador MIDI](docs/midi.md) — footswitches, knobs, pedales de expresión; [cualquier comando](docs/midi-commands.md) es mapeable por MIDI

### Para desarrolladores

- [Architecture](docs/architecture.md) · [Architecture (a fondo)](docs/development/architecture.md) — mapa de crates, capas, design patterns
- [Building](docs/development/building.md) — guía de build completa, incluyendo el motor NAM y Docker
- [Creating Blocks](docs/development/creating-blocks.md) — cómo añadir nuevos modelos de audio
- [Audio Backends](docs/development/audio-backends.md) — internos de Native, NAM, IR y LV2
- [File Organization](docs/development/file-organization.md) — dónde vive cada cosa, límites de LOC
- [Gitflow](docs/development/gitflow.md) — issues, branches, commits, workspace
- [Quality Gate](docs/development/quality-gate.md) — el gate comparativo único de CI
- [Testing](docs/testing.md) — cobertura, convenciones, comandos
- [Scripts](docs/scripts.md) — build/deploy, flujo `.deb` → Orange Pi
- [Formato del proyecto](docs/projects/project-openrig-format.md) — el modelo de proyecto `.openrig`
- [Arquitectura de la GUI](docs/gui/README.md) · [Graph view](docs/gui/graph-view.md) — internos de la ventana desktop ([diseño de ventana](docs/gui/2026-03-20-desktop-window-architecture.md) · [plan](docs/gui/2026-03-20-desktop-window-implementation-plan.md))
- [Contrato del backend](docs/backend/current-contract.md) · [Catálogo de modelos nativos](docs/backend/native-model-catalog.md) · [Referencia de efectos MK-300](docs/backend/mk-300-v69-effects-reference.md)
- [Idiomas (i18n)](docs/i18n.md) — framework de traducción de la UI, añadir un locale
- [Hardware](docs/hardware.md) · [Deploy Orange Pi](docs/hardware/orange-pi-deploy.md) — build de la placa del pedalboard y deploy de la imagen
- [ADRs](docs/adr/0001-project-model.md) — registros de decisión de arquitectura ([enrutamiento de device](docs/adr/0002-device-routing-and-validation.md))

## Contribuir

OpenRig es abierto por intención — las contribuciones son bienvenidas y la arquitectura está pensada para que sean tratables. El procesamiento de audio está separado por tipo de bloque, así cada modelo es totalmente dueño de su crate, con cero acoplamiento entre capturas brand-específicas y el resto del sistema. El proyecto sigue [Gitflow](https://nvie.com/posts/a-successful-git-branching-model/) con estándares estrictos de calidad: cero warnings, cero acoplamiento, single source of truth.

Mira [CONTRIBUTING.md](CONTRIBUTING.md) para branching, commits, PRs y estándares de código.

## Roadmap

Cada item abierto debajo está rastreado como una [issue de GitHub](https://github.com/jpfaria/OpenRig/issues) — ahí viven el progreso, la discusión de diseño y los PRs. Star o watch al repo para seguir.

### Hoy

- [x] App standalone para **macOS** (Apple Silicon + Intel), **Linux** (x86_64 + aarch64) y **Windows** (x86_64) — cinco targets de plataforma desde un único codebase
- [x] **Cadenas verdaderamente paralelas** — cada input es un runtime de audio aislado, sin búferes compartidos, sin locks contendidos, sin picos de CPU entre streams
- [x] **[560+ modelos](docs/user-guide/blocks-reference.md#model-id-quick-reference)** en 16 tipos de bloque, con **cuatro backends de audio** (Native DSP, NAM, IR, LV2) coexistiendo en el mismo grafo en tiempo real
- [x] **I/O de audio nativo en cada plataforma** — Core Audio (macOS), ALSA + JACK (Linux), WASAPI (Windows)
- [x] **Afinador cromático en tiempo real** como bloque de primera clase — colócalo en cualquier punto de la cadena
- [x] **Analizador de espectro en tiempo real** como bloque de primera clase — ve lo que oyes
- [x] **UI multi-idioma** — 9 idiomas hoy: inglés (`en-US`), portugués (`pt-BR`), español (`es-ES`), francés (`fr-FR`), alemán (`de-DE`), japonés (`ja-JP`), coreano (`ko-KR`), chino simplificado (`zh-CN`) e hindi (`hi-IN`); el framework de i18n está listo para contribuciones de la comunidad
- [x] **Filtrado por instrumento por cadena** — guitarra eléctrica, guitarra acústica, bajo, voz, teclado, batería o genérico — solo aparecen los bloques relevantes
- [x] **Múltiples bloques de I/O por cadena** con configuración independiente de dispositivo y canal por bloque
- [x] **Bypass por bloque** — cualquier bloque puede activarse o desactivarse en vivo sin reconstruir la cadena
- [x] **Loaders de IR y NAM del usuario** — coloca cualquier `.wav` de respuesta al impulso o captura `.nam` en la cadena en runtime
- [x] **Formato de preset YAML abierto** — diffeable, compartible por gist, scriptable; registry canónico de `MODEL_ID` documentado en [Blocks Reference](docs/user-guide/blocks-reference.md)
- [x] **Construcción de preset asistida por IA** — la skill [`openrig-tone-builder`](skills/openrig-tone-builder/SKILL.md) de Claude Code va con el repo y escribe presets completos a partir de una canción o un artista
- [x] **Adaptador de controlador MIDI / BLE-MIDI** — footswitches, knobs, pedal de expresión; **cualquier comando es mapeable por MIDI** (presets/escenas, selección de bloques, parámetros) ([#22](https://github.com/jpfaria/OpenRig/issues/22), ver [docs/midi-commands.md](docs/midi-commands.md))

### Features de escenario

- [ ] Snapshots / escenas ([#321](https://github.com/jpfaria/OpenRig/issues/321))
- [ ] Setlist / modo live performance ([#325](https://github.com/jpfaria/OpenRig/issues/325))
- [ ] Looper, multi-capa ([#323](https://github.com/jpfaria/OpenRig/issues/323))
- [ ] Backing tracks / reproductor de audio ([#324](https://github.com/jpfaria/OpenRig/issues/324))
- [ ] Mapeado de pedal de expresión por MIDI CC ([#326](https://github.com/jpfaria/OpenRig/issues/326))
- [ ] Tap tempo global / BPM por preset ([#322](https://github.com/jpfaria/OpenRig/issues/322))
- [ ] Routing paralelo / splits de cadena ([#328](https://github.com/jpfaria/OpenRig/issues/328))
- [ ] A/B compare ([#327](https://github.com/jpfaria/OpenRig/issues/327))
- [ ] Master mixer por stream ([#344](https://github.com/jpfaria/OpenRig/issues/344))

### Fundación sonora

- [ ] Reescritura de DSP nativo de cada tipo de bloque desde primeros principios, papers y sin dependencia de capturas externas ([#380](https://github.com/jpfaria/OpenRig/issues/380) umbrella, con sub-issues [#381–#392](https://github.com/jpfaria/OpenRig/issues?q=is%3Aopen+is%3Aissue+label%3Acore+38))
- [ ] Modelos manuales por componente para los amps benchmark de OpenRig ([#347](https://github.com/jpfaria/OpenRig/issues/347))
- [ ] Generadores NAM → nativo para amps y preamps ([#282](https://github.com/jpfaria/OpenRig/issues/282), [#283](https://github.com/jpfaria/OpenRig/issues/283))
- [ ] Generadores IR → nativo para cabinets y cuerpos acústicos ([#284](https://github.com/jpfaria/OpenRig/issues/284), [#285](https://github.com/jpfaria/OpenRig/issues/285))
- [ ] Asistente de plugin del usuario para import NAM / IR ([#287](https://github.com/jpfaria/OpenRig/issues/287))

### Ecosistema y remoto

- [ ] Servidor gRPC para control remoto de cadena por la red
- [ ] App de móvil y tablet como superficie de control por músico
- [ ] Form factor pedalera — hardware tipo Orange Pi, Linux de baja latencia
- [ ] Pedalera como terminal — controlador USB / Bluetooth / red para nodos remotos
- [ ] Proyectos multi-músico en un único nodo
- [ ] `openrig-cli` — cliente CLI scriptable por gRPC ([#298](https://github.com/jpfaria/OpenRig/issues/298))
- [ ] OpenRig Hub — marketplace comunitario de plugins ([#309](https://github.com/jpfaria/OpenRig/issues/309))
- [ ] Plugin VST3 / AU

### Expansión de catálogo

Los 560+ modelos actuales son la semilla. La expansión por bloque está rastreada bajo la [label `planned`](https://github.com/jpfaria/OpenRig/issues?q=is%3Aopen+is%3Aissue+label%3Aplanned), incluyendo un pipeline comunitario de import LV2/VST3 ([#372](https://github.com/jpfaria/OpenRig/issues/372), [#374](https://github.com/jpfaria/OpenRig/issues/374), [#379](https://github.com/jpfaria/OpenRig/issues/379)) y la integración masiva de Airwindows ([#373](https://github.com/jpfaria/OpenRig/issues/373)).

## Licencia

OpenRig se distribuye bajo la [GNU General Public License v3.0](LICENSE) — el rig que construyes es tuyo. Para siempre.
