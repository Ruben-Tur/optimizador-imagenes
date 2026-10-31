//! Punto de entrada de `afinar`.

use std::io::{self, IsTerminal};
use std::process::ExitCode;

use clap::Parser;

use afinar::cli::Cli;
use afinar::informe::{imprimir_json, Terminal};
use afinar::lote::{self, Informe, Resultado};
use afinar::presentacion::{pinta, ROJO};

/// Código de salida cuando alguna imagen no se ha podido convertir.
const SALIDA_PARCIAL: u8 = 3;

fn main() -> ExitCode {
    let cli = Cli::parse();
    match ejecutar(&cli) {
        Ok(codigo) => codigo,
        Err(error) => {
            let prefijo = pinta(ROJO, "error:");
            anstream::eprintln!("{prefijo} {error:#}");
            ExitCode::FAILURE
        }
    }
}

fn ejecutar(cli: &Cli) -> anyhow::Result<ExitCode> {
    let opciones = cli.opciones();
    let plan = lote::planificar(&opciones)?;
    if cli.json {
        let informe = lote::ejecutar(plan, &opciones, &|_: &Resultado| {})?;
        imprimir_json(&informe, &opciones)?;
        return Ok(codigo(&informe));
    }
    let terminal = Terminal::new(&plan, &opciones, io::stdout().is_terminal());
    if plan.total() == 0 {
        terminal.sin_imagenes(&opciones.entrada);
        return Ok(ExitCode::SUCCESS);
    }
    terminal.cabecera(&plan, &opciones);
    let informe = lote::ejecutar(plan, &opciones, &|r: &Resultado| terminal.avisar(r))?;
    terminal.resumen(&informe);
    Ok(codigo(&informe))
}

/// 0 si todo ha ido bien; 3 si alguna imagen ha fallado (el resto, sí).
fn codigo(informe: &Informe) -> ExitCode {
    if informe.cuenta().fallidas > 0 {
        ExitCode::from(SALIDA_PARCIAL)
    } else {
        ExitCode::SUCCESS
    }
}
