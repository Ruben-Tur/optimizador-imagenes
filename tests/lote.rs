//! Pruebas de extremo a extremo del lote, con imágenes generadas al vuelo en
//! una carpeta temporal.

use std::fs;
use std::path::{Path, PathBuf};

use afinar::formato::Formato;
use afinar::lote::{self, Estado, Informe, Opciones, Resultado};
use image::{Rgb, RgbImage, Rgba, RgbaImage};
use tempfile::TempDir;

/// Degradado con ruido: se parece más a una foto que un color liso y el PNG
/// pesa lo suficiente como para notar el ahorro.
fn crear_imagen(ruta: &Path, ancho: u32, alto: u32) {
    let mut semilla: u32 = 12_345;
    let imagen = RgbImage::from_fn(ancho, alto, |x, y| {
        semilla = semilla.wrapping_mul(1_103_515_245).wrapping_add(12_345);
        let ruido = (semilla >> 24) as u8 / 8;
        let rojo = (x * 200 / ancho) as u8;
        let verde = (y * 200 / alto) as u8;
        Rgb([rojo + ruido, verde + ruido, 120 + ruido])
    });
    imagen.save(ruta).unwrap();
}

fn opciones_base(entrada: &Path, salida: &Path) -> Opciones {
    Opciones {
        entrada: entrada.to_path_buf(),
        salida: salida.to_path_buf(),
        formatos: vec![Formato::Webp, Formato::Avif],
        ancho_max: None,
        calidad: 80,
        hilos: 2,
        forzar: false,
        simular: false,
    }
}

fn convertir(opciones: &Opciones) -> Informe {
    let plan = lote::planificar(opciones).unwrap();
    lote::ejecutar(plan, opciones, &|_: &Resultado| {}).unwrap()
}

/// Medidas de un WebP generado, que además tiene que poder abrirse.
fn medidas_webp(ruta: &Path) -> (u32, u32) {
    let imagen = image::open(ruta).unwrap();
    (imagen.width(), imagen.height())
}

/// Un AVIF empieza con la caja `ftyp` y la marca `avif`.
fn es_avif(ruta: &Path) -> bool {
    let datos = fs::read(ruta).unwrap();
    datos.get(4..12) == Some(b"ftypavif".as_slice())
}

fn tamano(ruta: &Path) -> u64 {
    fs::metadata(ruta).unwrap().len()
}

/// Carpeta temporal con `fotos/` dentro; devuelve también `optimizadas/`.
fn preparar() -> (TempDir, PathBuf, PathBuf) {
    let dir = TempDir::new().unwrap();
    let entrada = dir.path().join("fotos");
    let salida = dir.path().join("optimizadas");
    fs::create_dir_all(entrada.join("viajes")).unwrap();
    (dir, entrada, salida)
}

#[test]
fn convierte_replica_la_estructura_y_ahorra() {
    let (_dir, entrada, salida) = preparar();
    let portada = entrada.join("portada.png");
    crear_imagen(&portada, 320, 200);
    crear_imagen(&entrada.join("viajes/playa.jpg"), 240, 160);
    let antes = fs::read(&portada).unwrap();

    let informe = convertir(&opciones_base(&entrada, &salida));

    assert_eq!(informe.cuenta().convertidas, 2);
    assert_eq!(informe.cuenta().fallidas, 0);
    let webp = salida.join("portada.webp");
    let avif = salida.join("portada.avif");
    let playa = salida.join("viajes").join("playa");
    assert_eq!(medidas_webp(&webp), (320, 200));
    assert_eq!(medidas_webp(&playa.with_extension("webp")), (240, 160));
    assert!(es_avif(&avif));
    assert!(es_avif(&playa.with_extension("avif")));
    assert!(tamano(&webp) < tamano(&portada));
    assert!(tamano(&avif) < tamano(&portada));
    // El original no se toca.
    assert_eq!(fs::read(portada).unwrap(), antes);
}

#[test]
fn redimensiona_sin_ampliar_nunca() {
    let (_dir, entrada, salida) = preparar();
    crear_imagen(&entrada.join("grande.png"), 640, 400);
    crear_imagen(&entrada.join("pequena.png"), 200, 100);
    let mut opciones = opciones_base(&entrada, &salida);
    opciones.ancho_max = Some(320);
    opciones.formatos = vec![Formato::Webp];

    convertir(&opciones);

    let grande = salida.join("grande.webp");
    let pequena = salida.join("pequena.webp");
    assert_eq!(medidas_webp(&grande), (320, 200));
    assert_eq!(medidas_webp(&pequena), (200, 100));
    assert!(!salida.join("grande.avif").exists());
}

#[test]
fn un_archivo_roto_no_detiene_el_lote() {
    let (_dir, entrada, salida) = preparar();
    crear_imagen(&entrada.join("buena.png"), 160, 120);
    fs::write(entrada.join("rota.jpg"), b"esto no es un JPEG").unwrap();

    let informe = convertir(&opciones_base(&entrada, &salida));

    let cuenta = informe.cuenta();
    assert_eq!((cuenta.convertidas, cuenta.fallidas), (1, 1));
    let rota = &informe.resultados[1];
    assert_eq!(rota.relativa, Path::new("rota.jpg"));
    let Estado::Fallida(fallo) = &rota.estado else {
        panic!("rota.jpg tendría que haber fallado");
    };
    assert!(fallo.mensaje.starts_with("no es una imagen válida"));
    assert!(fallo.detalle.is_some());
    assert!(salida.join("buena.webp").exists());
    assert!(!salida.join("rota.webp").exists());
}

#[test]
fn salta_lo_que_esta_al_dia_salvo_con_forzar() {
    let (_dir, entrada, salida) = preparar();
    crear_imagen(&entrada.join("foto.png"), 120, 80);
    let opciones = opciones_base(&entrada, &salida);

    convertir(&opciones);
    let segunda = convertir(&opciones);
    assert_eq!(segunda.cuenta().al_dia, 1);
    assert_eq!(segunda.cuenta().convertidas, 0);

    let mut forzada = opciones.clone();
    forzada.forzar = true;
    assert_eq!(convertir(&forzada).cuenta().convertidas, 1);
}

#[test]
fn completa_solo_los_formatos_que_faltan() {
    let (_dir, entrada, salida) = preparar();
    crear_imagen(&entrada.join("foto.png"), 120, 80);
    let mut solo_webp = opciones_base(&entrada, &salida);
    solo_webp.formatos = vec![Formato::Webp];
    convertir(&solo_webp);

    let informe = convertir(&opciones_base(&entrada, &salida));

    let Estado::Convertida(conversiones) = &informe.resultados[0].estado else {
        panic!("foto.png tendría que haberse convertido");
    };
    assert_eq!(conversiones.len(), 1);
    assert_eq!(conversiones[0].formato, Formato::Avif);
}

#[test]
fn simular_no_escribe_nada() {
    let (_dir, entrada, salida) = preparar();
    crear_imagen(&entrada.join("foto.png"), 120, 80);
    let mut opciones = opciones_base(&entrada, &salida);
    opciones.simular = true;

    let informe = convertir(&opciones);

    assert_eq!(informe.cuenta().pendientes, 1);
    assert!(!salida.exists());
}

#[test]
fn la_salida_dentro_de_la_entrada_no_se_vuelve_a_leer() {
    let (_dir, entrada, _) = preparar();
    crear_imagen(&entrada.join("foto.png"), 120, 80);
    let salida = entrada.join("optimizadas");

    convertir(&opciones_base(&entrada, &salida));
    let segunda = convertir(&opciones_base(&entrada, &salida));

    assert_eq!(segunda.resultados.len(), 1);
    assert_eq!(segunda.cuenta().al_dia, 1);
}

#[test]
fn nunca_escribe_sobre_los_originales() {
    let (dir, entrada, _) = preparar();
    crear_imagen(&entrada.join("foto.png"), 120, 80);
    let misma = opciones_base(&entrada, &entrada);
    let contenedora = opciones_base(&entrada, dir.path());
    assert!(lote::planificar(&misma).is_err());
    assert!(lote::planificar(&contenedora).is_err());
}

#[test]
fn dos_originales_con_el_mismo_resultado_se_avisan() {
    let (_dir, entrada, salida) = preparar();
    crear_imagen(&entrada.join("foto.jpg"), 64, 48);
    crear_imagen(&entrada.join("foto.png"), 64, 48);

    let informe = convertir(&opciones_base(&entrada, &salida));

    assert_eq!(informe.cuenta().convertidas, 1);
    assert_eq!(informe.cuenta().fallidas, 1);
}

#[test]
fn conserva_la_transparencia() {
    let (_dir, entrada, salida) = preparar();
    let mut logo = RgbaImage::from_pixel(64, 64, Rgba([240, 165, 143, 255]));
    logo.put_pixel(0, 0, Rgba([0, 0, 0, 0]));
    logo.save(entrada.join("logo.png")).unwrap();
    let mut opciones = opciones_base(&entrada, &salida);
    opciones.formatos = vec![Formato::Webp];

    convertir(&opciones);

    let webp = image::open(salida.join("logo.webp")).unwrap();
    assert!(webp.color().has_alpha());
}
