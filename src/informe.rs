//! Cómo se cuenta el resultado: informe en la terminal mientras avanza el
//! lote, o un resumen en JSON para scripts.

use std::io::{self, Write};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use serde::Serialize;

use crate::formato::Formato;
use crate::lote::{Conversion, Cuenta, Estado, Informe, Opciones, Plan, Resultado, Total};
use crate::presentacion::{self, pinta, plural, ACENTO, NEGRITA, ROJO, TENUE, VERDE};
use crate::rutas;

/// Ancho de la barra de progreso, en caracteres.
const ANCHO_BARRA: usize = 30;

/// Límites del ancho de la columna de nombres.
const NOMBRE_MINIMO: usize = 12;
const NOMBRE_MAXIMO: usize = 40;

/// Informe para personas, que se va escribiendo mientras avanza el lote.
///
/// Las imágenes terminan en cualquier orden y desde varios hilos; un cerrojo
/// garantiza que las líneas no se mezclen y que la barra se redibuje bien.
pub struct Terminal {
    ancho_nombre: usize,
    total: usize,
    barra: bool,
    progreso: Mutex<Progreso>,
}

#[derive(Default)]
struct Progreso {
    hechas: usize,
}

impl Terminal {
    /// `barra` indica si la salida es una terminal donde tiene sentido
    /// dibujar una barra de progreso.
    pub fn new(plan: &Plan, opciones: &Opciones, barra: bool) -> Terminal {
        let largo = plan.nombre_mas_largo();
        let total = plan.por_convertir();
        Terminal {
            ancho_nombre: largo.clamp(NOMBRE_MINIMO, NOMBRE_MAXIMO),
            total,
            barra: barra && !opciones.simular && total > 0,
            progreso: Mutex::new(Progreso::default()),
        }
    }

    /// Aviso cuando la carpeta no tiene ninguna imagen que leer.
    pub fn sin_imagenes(&self, entrada: &Path) {
        let carpeta = presentacion::seguro(&entrada.display().to_string());
        let aviso = format!("No hay imágenes JPG, PNG ni WebP en «{carpeta}».");
        escribir(&aviso);
    }

    /// Primera línea (cuántas imágenes y con cuántos hilos) y los problemas
    /// encontrados antes de empezar, como carpetas ilegibles.
    pub fn cabecera(&self, plan: &Plan, opciones: &Opciones) {
        let imagenes = plural(plan.total(), "imagen", "imágenes");
        let detalle = if opciones.simular {
            "simulación, no se escribe nada".to_string()
        } else {
            plural(opciones.hilos, "hilo", "hilos")
        };
        let texto = format!("{imagenes} · {detalle}");
        escribir(&pinta(TENUE, &texto));
        if opciones.simular {
            return;
        }
        for resultado in plan.resueltos() {
            if let Estado::Fallida(_) = resultado.estado {
                for linea in self.lineas_de(resultado) {
                    escribir(&linea);
                }
            }
        }
    }

    /// Se llama desde los hilos de trabajo al terminar cada imagen.
    pub fn avisar(&self, resultado: &Resultado) {
        let mut progreso = self.bloquear();
        progreso.hechas += 1;
        let mut salida = anstream::stdout();
        if self.barra {
            borrar_barra(&mut salida);
        }
        for linea in self.lineas_de(resultado) {
            let _ = writeln!(salida, "{linea}");
        }
        if self.barra {
            dibujar_barra(&mut salida, progreso.hechas, self.total);
        }
    }

    /// Cierra la barra y escribe los totales.
    pub fn resumen(&self, informe: &Informe) {
        let mut salida = anstream::stdout();
        if informe.simulado {
            for resultado in &informe.resultados {
                for linea in self.lineas_de(resultado) {
                    let _ = writeln!(salida, "{linea}");
                }
            }
        }
        if self.barra {
            borrar_barra(&mut salida);
            dibujar_barra(&mut salida, self.total, self.total);
            let _ = writeln!(salida);
        }
        let _ = writeln!(salida);
        for linea in lineas_resumen(informe) {
            let _ = writeln!(salida, "{linea}");
        }
    }

    fn bloquear(&self) -> MutexGuard<'_, Progreso> {
        // Si otro hilo entró en pánico con el cerrojo, el contador sigue
        // siendo válido: se aprovecha en vez de propagar el pánico.
        match self.progreso.lock() {
            Ok(guarda) => guarda,
            Err(envenenado) => envenenado.into_inner(),
        }
    }

    /// Líneas que describen el resultado de una imagen.
    fn lineas_de(&self, resultado: &Resultado) -> Vec<String> {
        let ruta = presentacion::seguro(&rutas::legible(&resultado.relativa));
        let nombre = presentacion::ajustar(&ruta, self.ancho_nombre);
        let original = format!("{:>8}", presentacion::bytes(resultado.bytes));
        match &resultado.estado {
            Estado::Convertida(lista) => filas(&nombre, &original, resultado.bytes, lista),
            Estado::Pendiente(formatos) => vec![pendiente(&nombre, &original, formatos)],
            Estado::AlDia => vec![al_dia(&nombre, &original)],
            Estado::Fallida(fallo) => vec![fallida(&nombre, &fallo.mensaje)],
        }
    }
}

/// Escribe una línea completa; si la salida se ha cerrado (por ejemplo, con
/// `| head`) se ignora en vez de abortar.
fn escribir(linea: &str) {
    let _ = writeln!(anstream::stdout(), "{linea}");
}

fn borrar_barra(salida: &mut impl Write) {
    let blanco = " ".repeat(ANCHO_BARRA + 16);
    let _ = write!(salida, "\r{blanco}\r");
}

fn dibujar_barra(salida: &mut impl Write, hechas: usize, total: usize) {
    if total == 0 {
        return;
    }
    let llenas = (hechas * ANCHO_BARRA / total).min(ANCHO_BARRA);
    let barra = pinta(ACENTO, &"━".repeat(llenas));
    let resto = pinta(TENUE, &"━".repeat(ANCHO_BARRA - llenas));
    let _ = write!(salida, "\r{barra}{resto}  {hechas}/{total}");
    let _ = salida.flush();
}

/// Una fila por formato; el nombre y el tamaño original, solo en la primera.
fn filas(nombre: &str, original: &str, bytes: u64, lista: &[Conversion]) -> Vec<String> {
    let hueco_nombre = " ".repeat(nombre.chars().count());
    let hueco_original = " ".repeat(original.chars().count());
    let mut lineas = Vec::with_capacity(lista.len());
    for (indice, conversion) in lista.iter().enumerate() {
        let (izquierda, previo) = if indice == 0 {
            (nombre, original)
        } else {
            (hueco_nombre.as_str(), hueco_original.as_str())
        };
        lineas.push(fila(izquierda, previo, bytes, conversion));
    }
    lineas
}

/// `portada.jpg   3,80 MB  → webp   412 KB   -89 %`
fn fila(nombre: &str, previo: &str, original: u64, conversion: &Conversion) -> String {
    let nuevo = presentacion::bytes(conversion.bytes);
    let variacion = presentacion::variacion(original, conversion.bytes);
    let ahorra = conversion.bytes < original;
    let estilo = if ahorra { VERDE } else { ACENTO };
    let previo = pinta(TENUE, previo);
    let flecha = pinta(ACENTO, "→");
    let destino = format!("{} {nuevo:>8}", conversion.formato);
    let cambio = pinta(estilo, &format!("{variacion:>6}"));
    format!("{nombre}  {previo}  {flecha} {destino}  {cambio}")
}

/// `portada.jpg   3,80 MB  → webp, avif` (al simular)
fn pendiente(nombre: &str, original: &str, formatos: &[Formato]) -> String {
    let lista: Vec<&str> = formatos.iter().map(|f| f.extension()).collect();
    let lista = lista.join(", ");
    let previo = pinta(TENUE, original);
    let flecha = pinta(ACENTO, "→");
    format!("{nombre}  {previo}  {flecha} {lista}")
}

/// `portada.jpg   3,80 MB  · al día` (al simular)
fn al_dia(nombre: &str, original: &str) -> String {
    let resto = pinta(TENUE, &format!("{original}  · al día"));
    format!("{nombre}  {resto}")
}

/// `rota.jpg   ✗ no es una imagen válida o está dañada`
fn fallida(nombre: &str, mensaje: &str) -> String {
    let marca = pinta(ROJO, "✗");
    let mensaje = presentacion::seguro(mensaje);
    format!("{nombre}  {marca} {mensaje}")
}

/// Totales por formato y recuento final.
fn lineas_resumen(informe: &Informe) -> Vec<String> {
    let cuenta = informe.cuenta();
    let mut lineas = Vec::new();
    if informe.simulado {
        lineas.push(linea_simulacion(&cuenta));
        return lineas;
    }
    for (formato, total) in informe.totales() {
        lineas.push(linea_total(formato, &total));
    }
    lineas.push(linea_cuenta(&cuenta, informe.duracion));
    if cuenta.convertidas == 0 && cuenta.al_dia > 0 {
        let pista = "  Nada que hacer: usa --forzar para volver a convertirlas.";
        lineas.push(pinta(TENUE, pista));
    }
    lineas
}

/// `✓ webp  1,02 GB → 96,4 MB (-91 %)`
fn linea_total(formato: Formato, total: &Total) -> String {
    let marca = pinta(VERDE, "✓");
    let antes = presentacion::bytes(total.originales);
    let despues = pinta(NEGRITA, &presentacion::bytes(total.nuevos));
    let flecha = pinta(ACENTO, "→");
    let variacion = presentacion::variacion(total.originales, total.nuevos);
    let cambio = pinta(TENUE, &format!("({variacion})"));
    format!("{marca} {formato}  {antes} {flecha} {despues} {cambio}")
}

/// `  5 convertidas · 1 con error · 4,8 s`
fn linea_cuenta(cuenta: &Cuenta, duracion: Duration) -> String {
    let separador = pinta(TENUE, " · ");
    let convertidas = plural(cuenta.convertidas, "convertida", "convertidas");
    let mut partes = vec![pinta(TENUE, &convertidas)];
    if cuenta.al_dia > 0 {
        let al_dia = format!("{} al día", cuenta.al_dia);
        partes.push(pinta(TENUE, &al_dia));
    }
    if cuenta.fallidas > 0 {
        let errores = plural(cuenta.fallidas, "con error", "con errores");
        partes.push(pinta(ROJO, &errores));
    }
    partes.push(pinta(TENUE, &presentacion::duracion(duracion)));
    format!("  {}", partes.join(separador.as_str()))
}

/// `→ 5 por convertir · 1 al día` y el recordatorio de que no se ha escrito.
fn linea_simulacion(cuenta: &Cuenta) -> String {
    let flecha = pinta(ACENTO, "→");
    let mut texto = format!("{flecha} {} por convertir", cuenta.pendientes);
    if cuenta.al_dia > 0 {
        texto.push_str(&format!(" · {} al día", cuenta.al_dia));
    }
    if cuenta.fallidas > 0 {
        let errores = plural(cuenta.fallidas, "con error", "con errores");
        texto.push_str(&format!(" · {}", pinta(ROJO, &errores)));
    }
    let aviso = pinta(TENUE, "  Simulación: no se ha escrito nada.");
    format!("{texto}\n{aviso}")
}

/// Resumen para scripts: siempre el mismo esquema, rutas con «/».
#[derive(Serialize)]
struct ResumenJson {
    version: &'static str,
    entrada: String,
    salida: String,
    simulacion: bool,
    hilos: usize,
    duracion_ms: u64,
    resumen: Cuenta,
    formatos: Vec<FormatoJson>,
    archivos: Vec<ArchivoJson>,
}

#[derive(Serialize)]
struct FormatoJson {
    formato: String,
    archivos: usize,
    bytes_originales: u64,
    bytes_nuevos: u64,
    ahorro_pct: f64,
}

#[derive(Serialize)]
struct ArchivoJson {
    ruta: String,
    bytes: u64,
    estado: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    salidas: Vec<SalidaJson>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pendientes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detalle: Option<String>,
}

#[derive(Serialize)]
struct SalidaJson {
    formato: String,
    ruta: String,
    bytes: u64,
    ancho: u32,
    alto: u32,
    ahorro_pct: f64,
}

/// Imprime el resumen en JSON en la salida estándar.
pub fn imprimir_json(informe: &Informe, opciones: &Opciones) -> anyhow::Result<()> {
    let resumen = resumen_json(informe, opciones);
    let texto = serde_json::to_string_pretty(&resumen)?;
    let mut salida = io::stdout().lock();
    writeln!(salida, "{texto}")?;
    Ok(())
}

fn resumen_json(informe: &Informe, opciones: &Opciones) -> ResumenJson {
    let mut formatos = Vec::new();
    for (formato, total) in informe.totales() {
        formatos.push(formato_json(formato, &total));
    }
    let mut archivos = Vec::new();
    for resultado in &informe.resultados {
        archivos.push(archivo_json(resultado));
    }
    let milisegundos = informe.duracion.as_millis();
    ResumenJson {
        version: env!("CARGO_PKG_VERSION"),
        entrada: opciones.entrada.display().to_string(),
        salida: opciones.salida.display().to_string(),
        simulacion: informe.simulado,
        hilos: informe.hilos,
        duracion_ms: u64::try_from(milisegundos).unwrap_or(u64::MAX),
        resumen: informe.cuenta(),
        formatos,
        archivos,
    }
}

fn formato_json(formato: Formato, total: &Total) -> FormatoJson {
    FormatoJson {
        formato: formato.to_string(),
        archivos: total.archivos,
        bytes_originales: total.originales,
        bytes_nuevos: total.nuevos,
        ahorro_pct: ahorro(total.originales, total.nuevos),
    }
}

fn archivo_json(resultado: &Resultado) -> ArchivoJson {
    let mut archivo = ArchivoJson {
        ruta: rutas::legible(&resultado.relativa),
        bytes: resultado.bytes,
        estado: "al_dia",
        salidas: Vec::new(),
        pendientes: Vec::new(),
        error: None,
        detalle: None,
    };
    match &resultado.estado {
        Estado::Convertida(lista) => {
            archivo.estado = "convertida";
            let mut salidas = Vec::with_capacity(lista.len());
            for conversion in lista {
                salidas.push(salida_json(resultado, conversion));
            }
            archivo.salidas = salidas;
        }
        Estado::AlDia => {}
        Estado::Pendiente(formatos) => {
            archivo.estado = "pendiente";
            archivo.pendientes = formatos.iter().map(|f| f.to_string()).collect();
        }
        Estado::Fallida(fallo) => {
            archivo.estado = "fallida";
            archivo.error = Some(fallo.mensaje.clone());
            archivo.detalle = fallo.detalle.clone();
        }
    }
    archivo
}

fn salida_json(resultado: &Resultado, conversion: &Conversion) -> SalidaJson {
    let extension = conversion.formato.extension();
    let relativa = resultado.relativa.with_extension(extension);
    SalidaJson {
        formato: conversion.formato.to_string(),
        ruta: rutas::legible(&relativa),
        bytes: conversion.bytes,
        ancho: conversion.ancho,
        alto: conversion.alto,
        ahorro_pct: ahorro(resultado.bytes, conversion.bytes),
    }
}

/// Ahorro en tanto por ciento con un decimal: positivo si pesa menos.
fn ahorro(original: u64, nuevo: u64) -> f64 {
    let ahorro = -presentacion::porcentaje(original, nuevo);
    // Sumar 0.0 convierte un posible «-0.0» en «0.0» en el JSON.
    (ahorro * 10.0).round() / 10.0 + 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ahorro_positivo_con_un_decimal() {
        assert_eq!(ahorro(1_000, 110).to_string(), "89");
        assert_eq!(ahorro(3_000, 1_000).to_string(), "66.7");
        assert_eq!(ahorro(1_000, 1_200).to_string(), "-20");
        assert_eq!(ahorro(1_000, 1_000).to_string(), "0");
    }

    #[test]
    fn la_fila_alinea_columnas_sin_colores() {
        let conversion = Conversion {
            formato: Formato::Webp,
            ruta: "x.webp".into(),
            bytes: 412_000,
            ancho: 1600,
            alto: 1067,
        };
        let linea = fila("portada.jpg ", " 3,80 MB", 3_800_000, &conversion);
        let limpia = anstream::adapter::strip_str(&linea).to_string();
        let esperada = "portada.jpg    3,80 MB  → webp   412 KB   -89 %";
        assert_eq!(limpia, esperada);
    }
}
