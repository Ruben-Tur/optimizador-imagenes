# Optimizador de imágenes por lotes (`afinar`)

`afinar` es una herramienta de línea de comandos escrita en Rust que convierte carpetas enteras de imágenes JPG, PNG y WebP a **WebP y AVIF** en paralelo, las redimensiona al ancho que le pidas y te dice cuánto has ahorrado, sin tocar nunca los originales.

Sirve para quien publica webs (WordPress, Astro, tiendas online...) y quiere que las imágenes, que suelen ser lo que más pesa de una página, ocupen hasta un 90 % menos y mejoren la carga y las Core Web Vitals. Se usa a mano o dentro de un script de despliegue.

```text
$ afinar ./fotos --formato webp,avif --ancho 1600
```

## Características

- **Convierte por lotes** una carpeta y todas sus subcarpetas (`.jpg`, `.jpeg`, `.png`, `.webp`) a WebP, AVIF o ambos.
- **Redimensiona** a un ancho máximo manteniendo la proporción, con filtro Lanczos. Nunca amplía una imagen pequeña.
- **Conserva los originales**: los resultados se escriben en otra carpeta que replica la estructura de la de entrada (`fotos/blog/tortilla.jpg` → `optimizadas/blog/tortilla.webp`).
- **En paralelo** con [Rayon]: reparte las imágenes entre todos los núcleos (`--hilos` para limitarlo).
- **Incremental**: salta las imágenes cuyo resultado ya existe y es más reciente que el original; `--forzar` las rehace.
- **Un archivo roto no para el lote**: se anota en el informe, el resto sigue y el código de salida indica que hubo fallos.
- **Informe claro**: tamaño original → nuevo y porcentaje ahorrado de cada archivo, totales por formato y tiempo. Respeta `NO_COLOR` y no pinta colores ni barra de progreso si la salida no es una terminal.
- **Para scripts**: `--json` imprime un resumen estructurado y `--simular` muestra qué haría sin escribir nada.
- **Corrige la orientación EXIF** de las fotos del móvil y conserva la transparencia de los PNG (y la descarta si no se usa).
- **Seguro ante archivos hostiles**: límites de tamaño y de memoria al decodificar, sin seguir enlaces simbólicos y sin escribir nunca fuera de la carpeta de salida (ver [Seguridad](#seguridad)).

## Tecnologías

| Parte | Tecnología |
|---|---|
| Lenguaje | Rust (edición 2021, versión estable; mínimo 1.88) |
| Línea de comandos | [clap] 4 con `derive`, ayuda y opciones en español |
| Paralelismo | [Rayon] |
| Lectura y redimensionado | [image] 0.25 (JPEG, PNG y WebP; filtro Lanczos3) |
| WebP con pérdida | [webp] 0.3 sobre libwebp, compilada desde su código con `cc` |
| AVIF | `image` con `ravif` y `rav1e` en Rust puro (no necesita `nasm`) |
| Recorrido de carpetas | [walkdir] |
| Colores en la terminal | [anstream] y [anstyle] (respetan `NO_COLOR` y las terminales de Windows) |
| Errores | [anyhow] y [thiserror] |
| JSON | [serde] y [serde_json] |
| Pruebas | `cargo test` con [tempfile] |
| Integración continua | GitHub Actions: `cargo fmt`, `cargo clippy`, pruebas en Linux y Windows, binarios y `cargo audit` |

## Requisitos

- Para usar un binario ya compilado: nada más (Linux o Windows x86_64).
- Para compilarlo: [Rust estable](https://rustup.rs) 1.88 o superior y un compilador de C (en Linux, `build-essential`; en Windows, las *Build Tools* de Visual Studio), porque libwebp se compila junto con el programa.

## Instalación

Con Rust instalado, desde la carpeta del proyecto:

```bash
cargo install --path .
```

Esto deja el ejecutable `afinar` en `~/.cargo/bin`. También puedes descargar el binario de Linux o de Windows desde la pestaña **Actions** del repositorio (artefactos `afinar-Linux` y `afinar-Windows` de la última ejecución correcta).

## Uso

```text
afinar <ENTRADA> [OPCIONES]
```

| Opción | Qué hace | Por defecto |
|---|---|---|
| `ENTRADA` | Carpeta con las imágenes originales | (obligatoria) |
| `-s, --salida <CARPETA>` | Carpeta de resultados | `optimizadas` |
| `-f, --formato <LISTA>` | Formatos separados por comas: `webp`, `avif` | `webp,avif` |
| `-a, --ancho <PX>` | Ancho máximo en píxeles (1 a 16 383); nunca amplía | el original |
| `-c, --calidad <1-100>` | Calidad: más alta, más nitidez y más peso | `80` |
| `--hilos <N>` | Hilos de trabajo (1 a 1024) | uno por núcleo |
| `--forzar` | Vuelve a convertir aunque el resultado esté al día | |
| `--simular` | Muestra qué haría sin escribir nada | |
| `--json` | Imprime un resumen en JSON en vez del informe | |
| `-h, --help` | Ayuda en español | |
| `-V, --version` | Versión y autor | |

Ejemplos:

```bash
# Todo a WebP y AVIF, con 1600 px de ancho como máximo
afinar ./fotos --formato webp,avif --ancho 1600 --salida ./optimizadas

# Solo WebP con un poco menos de calidad
afinar ./fotos --formato webp --calidad 75

# Ver qué haría sin escribir nada
afinar ./fotos --simular

# Dentro de un script de despliegue
afinar ./public/img --salida ./dist/img --json > informe-imagenes.json
```

### Lo que se ve en la terminal

Este es el formato exacto que imprime el programa (aquí sin colores, como cuando la salida se redirige a un archivo); los números son de ejemplo. En una terminal, las flechas y la barra salen en el color de acento (salmón, `#f0a58f`), los ahorros en verde y los errores en rojo. Las imágenes aparecen según van terminando, así que el orden puede cambiar de una ejecución a otra.

```text
$ afinar ./fotos --formato webp,avif --ancho 1600
5 imágenes · 8 hilos
portada-tienda.jpg         3,81 MB  → webp   412 KB   -89 %
                                    → avif   298 KB   -92 %
equipo-2025.png            6,10 MB  → webp   538 KB   -91 %
                                    → avif   402 KB   -93 %
blog/archivo-roto.jpg     ✗ no es una imagen válida o está dañada
producto-mesa.jpg          2,42 MB  → webp   286 KB   -88 %
                                    → avif   215 KB   -91 %
blog/receta-tortilla.jpg   1,87 MB  → webp   198 KB   -89 %
                                    → avif   151 KB   -92 %
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━  5/5

✓ webp  14,2 MB → 1,44 MB (-90 %)
✓ avif  14,2 MB → 1,07 MB (-93 %)
  4 convertidas · 1 con error · 3,4 s
```

Los tamaños usan unidades de 1000 bytes (KB, MB, GB) con tres cifras significativas y coma decimal. La barra de progreso solo aparece en una terminal.

### Resumen en JSON

Con `--json` no se imprime el informe, sino un único objeto JSON (rutas relativas y con `/` en todos los sistemas; `ahorro_pct` es positivo cuando el resultado pesa menos):

```json
{
  "version": "0.1.0",
  "entrada": "./fotos",
  "salida": "optimizadas",
  "simulacion": false,
  "hilos": 8,
  "duracion_ms": 3412,
  "resumen": { "imagenes": 5, "convertidas": 4, "al_dia": 0, "pendientes": 0, "fallidas": 1 },
  "formatos": [
    { "formato": "webp", "archivos": 4, "bytes_originales": 14206000, "bytes_nuevos": 1435000, "ahorro_pct": 89.9 }
  ],
  "archivos": [
    {
      "ruta": "blog/archivo-roto.jpg",
      "bytes": 48210,
      "estado": "fallida",
      "error": "no es una imagen válida o está dañada",
      "detalle": "Format error decoding Jpeg: ..."
    },
    {
      "ruta": "portada-tienda.jpg",
      "bytes": 3812000,
      "estado": "convertida",
      "salidas": [
        { "formato": "webp", "ruta": "portada-tienda.webp", "bytes": 412300, "ancho": 1600, "alto": 1067, "ahorro_pct": 89.2 }
      ]
    }
  ]
}
```

(El ejemplo está abreviado; la salida real va indentada con un campo por línea.) `estado` puede ser `convertida`, `al_dia`, `pendiente` (con `--simular`, junto a la lista `pendientes`) o `fallida`. `detalle` es el mensaje técnico de la biblioteca, en inglés.

### Códigos de salida

| Código | Significado |
|---|---|
| `0` | Todo convertido o ya al día |
| `1` | Error general antes de empezar (la carpeta de entrada no existe, la salida no es válida...) |
| `2` | Opciones no válidas (por ejemplo, `--calidad 0`) |
| `3` | El lote ha terminado, pero alguna imagen no se ha podido convertir |

## Compilar

```bash
cargo build --release        # binario en target/release/afinar(.exe)
cargo test                   # pruebas unitarias y de integración
cargo fmt --all -- --check   # formato
cargo clippy --all-targets -- -D warnings
```

- El perfil de publicación usa LTO y quita los símbolos, así que el binario es pequeño. En el perfil de desarrollo las dependencias se compilan optimizadas (`[profile.dev.package."*"]`), porque decodificar y codificar imágenes sin optimizar es muy lento y las pruebas tardarían minutos.
- No hace falta `nasm`: el codificador AVIF se compila en Rust puro. Es algo más lento que con ensamblador, pero no añade dependencias del sistema.
- **Cargo.lock**: al ser un programa (no una biblioteca), el `Cargo.lock` se versiona para que todas las compilaciones usen exactamente las mismas versiones, y CI compila con `--locked`. Para actualizar dependencias: `cargo update` y subir el lock nuevo.

## Seguridad

- **Originales intocables**: la carpeta de salida no puede ser la de entrada ni una que la contenga (los `.webp` de entrada acabarían sobrescritos). Puede estar dentro de la de entrada; en ese caso se excluye del recorrido para no volver a leer los resultados.
- **Sin salir de las carpetas**: no se siguen enlaces simbólicos al recorrer la entrada, se rechaza cualquier ruta relativa con `..` o raíces, y antes de escribir se comprueba que la carpeta de destino, ya resuelta, sigue dentro de la de salida.
- **Límites contra archivos hostiles** («bombas de descompresión»): como mucho 100 MB por archivo (se deja de leer al pasar el límite), 16 383 px de lado y 512 MiB de memoria por imagen, comprobados con la cabecera antes de decodificar.
- **Escritura atómica**: cada resultado se escribe en un temporal oculto de la misma carpeta y se renombra al final, así un corte nunca deja un archivo a medias que la siguiente ejecución daría por bueno.
- **Errores aislados**: un archivo dañado, sin permisos o que haga fallar a una biblioteca (incluido un pánico) se anota y el lote continúa.
- **Terminal a salvo**: los nombres de archivo se muestran sin caracteres de control, para que un nombre malicioso no pueda inyectar secuencias de escape.
- **Privacidad**: la herramienta no usa la red ni guarda datos propios. Los resultados no conservan los metadatos EXIF (incluida la ubicación GPS de las fotos del móvil); solo se aplica su orientación. Tampoco se copian los perfiles de color ICC, así que conviene que los originales estén en sRGB, que es lo habitual en la web.
- CI ejecuta `cargo audit` (RustSec) para avisar de vulnerabilidades conocidas en las dependencias.

## Estructura

```text
optimizador-imagenes/
├── Cargo.toml              Dependencias con versión y perfiles de compilación
├── rust-toolchain.toml     Rust estable con rustfmt y clippy
├── .github/workflows/ci.yml
├── src/
│   ├── main.rs             Punto de entrada: elige informe o JSON y el código de salida
│   ├── lib.rs              Módulos de la biblioteca (lo que se prueba)
│   ├── cli.rs              Opciones con clap, ayuda en español
│   ├── lote.rs             Planificación, ejecución en paralelo y recuentos
│   ├── imagen.rs           Lectura con límites, redimensionado y codificación WebP/AVIF
│   ├── rutas.rs            Recorrido, rutas de salida, validaciones y escritura atómica
│   ├── informe.rs          Informe en la terminal (barra, columnas) y resumen JSON
│   ├── presentacion.rs     Tamaños, porcentajes y tiempos en español; colores
│   ├── formato.rs          Formatos de salida (webp, avif)
│   └── error.rs            Errores de cada archivo, con mensajes en español
└── tests/
    ├── lote.rs             Conversión real con imágenes generadas en una carpeta temporal
    └── cli.rs              El binario: JSON, códigos de salida, salida sin colores, ayuda
```

## Pruebas

```bash
cargo test
```

- **Unitarias** (en cada módulo): formato de tamaños (`3,80 MB`, nunca `1000 KB`), porcentajes y tiempos; cálculo de medidas al redimensionar (proporción, redondeo, nunca ampliar); rutas de salida, rutas relativas peligrosas, extensiones, exclusión de la salida, enlaces simbólicos (Linux), escritura atómica y escritura fuera de la salida; lectura de opciones, rangos no válidos, ayuda y versión; límites contra imágenes enormes y archivos dañados; alineación de las columnas del informe.
- **Integración** (`tests/`): generan imágenes en una carpeta temporal, ejecutan la conversión y comprueban que los resultados existen, se pueden abrir, tienen las medidas esperadas, pesan menos que el PNG original y que este no cambia; que un archivo dañado se informa sin detener el lote; que se salta lo que está al día salvo con `--forzar`; que `--simular` no escribe; que se conserva la transparencia; y, ejecutando el binario, el JSON, los códigos de salida `0`, `1`, `2` y `3` y que no salen códigos de color cuando la salida no es una terminal.

Todas se ejecutan en GitHub Actions en Linux y en Windows.

---

Hecho por [Rubén Tur](https://github.com/Ruben-Tur).

[Rayon]: https://github.com/rayon-rs/rayon
[clap]: https://docs.rs/clap
[image]: https://docs.rs/image
[webp]: https://docs.rs/webp
[walkdir]: https://docs.rs/walkdir
[anstream]: https://docs.rs/anstream
[anstyle]: https://docs.rs/anstyle
[anyhow]: https://docs.rs/anyhow
[thiserror]: https://docs.rs/thiserror
[serde]: https://serde.rs
[serde_json]: https://docs.rs/serde_json
[tempfile]: https://docs.rs/tempfile
