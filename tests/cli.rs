//! Pruebas del binario: códigos de salida, JSON y salida sin colores.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use image::{Rgb, RgbImage};
use tempfile::TempDir;

fn afinar(carpeta: &Path, argumentos: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_afinar"))
        .args(argumentos)
        .current_dir(carpeta)
        .env_remove("NO_COLOR")
        .env_remove("CLICOLOR_FORCE")
        .output()
        .unwrap()
}

/// Carpeta temporal con `fotos/buena.png` y `fotos/rota.jpg`.
fn preparar() -> TempDir {
    let dir = TempDir::new().unwrap();
    let fotos = dir.path().join("fotos");
    fs::create_dir(&fotos).unwrap();
    let imagen = RgbImage::from_pixel(64, 48, Rgb([240, 165, 143]));
    imagen.save(fotos.join("buena.png")).unwrap();
    fs::write(fotos.join("rota.jpg"), b"no soy una imagen").unwrap();
    dir
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn json_y_codigo_de_salida_parcial() {
    let dir = preparar();
    let salida = afinar(dir.path(), &["fotos", "--json", "--formato", "webp"]);

    assert_eq!(salida.status.code(), Some(3));
    let json: serde_json::Value = serde_json::from_slice(&salida.stdout).unwrap();
    assert_eq!(json["resumen"]["convertidas"], 1);
    assert_eq!(json["resumen"]["fallidas"], 1);
    assert_eq!(json["archivos"][0]["ruta"], "buena.png");
    assert_eq!(json["archivos"][0]["salidas"][0]["ruta"], "buena.webp");
    assert_eq!(json["archivos"][1]["estado"], "fallida");
    assert!(dir.path().join("optimizadas/buena.webp").exists());
}

#[test]
fn informe_legible_y_sin_colores_fuera_de_una_terminal() {
    let dir = preparar();
    let salida = afinar(dir.path(), &["fotos", "--formato", "webp"]);

    assert_eq!(salida.status.code(), Some(3));
    let informe = texto(&salida.stdout);
    assert!(informe.starts_with("2 imágenes · "), "{informe}");
    assert!(informe.contains("→ webp"), "{informe}");
    assert!(informe.contains("✗ no es una imagen"), "{informe}");
    assert!(informe.contains("1 convertida"), "{informe}");
    assert!(informe.contains("1 con error"), "{informe}");
    assert!(!informe.contains('\u{1b}'), "{informe:?}");
}

#[test]
fn simular_no_crea_la_carpeta_de_salida() {
    let dir = preparar();
    let salida = afinar(dir.path(), &["fotos", "--simular"]);

    let informe = texto(&salida.stdout);
    assert!(informe.contains("Simulación: no se ha escrito nada."));
    assert!(!dir.path().join("optimizadas").exists());
}

#[test]
fn ayuda_y_version() {
    let dir = TempDir::new().unwrap();
    let ayuda = afinar(dir.path(), &["--help"]);
    assert_eq!(ayuda.status.code(), Some(0));
    let ayuda = texto(&ayuda.stdout);
    assert!(ayuda.contains("Uso: afinar <ENTRADA> [OPCIONES]"));

    let version = afinar(dir.path(), &["--version"]);
    assert!(texto(&version.stdout).contains("Hecho por Rubén Tur"));
}

#[test]
fn opciones_no_validas_y_errores_generales() {
    let dir = preparar();
    let calidad = afinar(dir.path(), &["fotos", "--calidad", "0"]);
    assert_eq!(calidad.status.code(), Some(2));

    let inexistente = afinar(dir.path(), &["no-existe"]);
    assert_eq!(inexistente.status.code(), Some(1));
    assert!(texto(&inexistente.stderr).contains("no se puede abrir"));
}

#[test]
fn carpeta_sin_imagenes() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("vacia")).unwrap();
    let salida = afinar(dir.path(), &["vacia"]);

    assert_eq!(salida.status.code(), Some(0));
    assert!(texto(&salida.stdout).starts_with("No hay imágenes"));
}
