//! Motor de obligaciones de SIGER.
//!
//! Aquí vive lo único que no se puede comprar hecho: qué días vence una
//! obligación y hasta cuándo hay para cumplirla. Sin base de datos, sin red y
//! sin interfaz — todo eso son detalles de entrega que cambian con el cliente.
//!
//! # Las tres invariantes
//!
//! 1. **Un vencimiento tiene dos fechas.** El día nominal lo fija la norma y no
//!    se mueve; el día límite es lo que queda tras resolver que el nominal no
//!    era hábil. Aquí son [`fechas::FechaNominal`] y [`fechas::FechaLimite`],
//!    tipos distintos, así que confundirlas no compila.
//! 2. **Ante la falta de un dato se falla.** Un país sin calendario de festivos
//!    es un [`calendario::ErrorCalendario`], no un calendario vacío: un motor de
//!    cumplimiento que calcula plazos sin festivos produce fechas plausibles y
//!    equivocadas, y eso no se nota nunca.
//! 3. **Lo que la fuente no dice no se inventa.** Un código de periodicidad al
//!    que le faltan datos devuelve [`patrones::SinTraducir`] con el motivo.
//!
//! # Cómo se sabe que es correcto
//!
//! `tests/oraculo.rs` compara este motor contra las fechas exactas que produjo
//! el motor anterior: 484 festivos entre 2020 y 2045 y 8 534 vencimientos de los
//! 80 patrones de una entidad real. No es un port «revisado»: es uno **medido**.

pub mod calendario;
pub mod compat;
pub mod fechas;
pub mod patrones;
pub mod recurrencia;

pub use calendario::{Calendario, ErrorCalendario};
pub use compat::{ErrorRegla, ReglaJson};
pub use fechas::{FechaLimite, FechaNominal, TipoAjuste, Vencimiento};
pub use recurrencia::{Ajuste, DiaDelMes, Especifico, Motor, Patron, Regla, Semanal};
