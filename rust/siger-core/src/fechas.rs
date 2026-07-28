//! Las dos fechas de un vencimiento, separadas por el sistema de tipos.
//!
//! En el motor anterior esto era un comentario largo pidiendo que no se
//! confundieran. Aquí son **dos tipos distintos**: pasar una donde va la otra no
//! compila. Es la diferencia entre documentar una regla y hacerla imposible de
//! romper, y esta concretamente ya se rompió tres veces:
//!
//!   1. la obligación cambiaba de identidad al ajustarse (un anual del 1 de
//!      enero con ajuste anticipado «pertenecía» al 31 de diciembre anterior, y
//!      por tanto al año anterior);
//!   2. con vigencia desde el 1 de enero, ese vencimiento caía fuera y
//!      **desaparecía**;
//!   3. para recuperarlo hacían falta dos mecanismos —barrer los meses vecinos y
//!      reconstruir el origen a posteriori— cuya única razón de ser era contener
//!      la confusión.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// El día que fija la norma o el contrato. **No se mueve nunca**, aunque sea
/// festivo, y es la identidad de la obligación: el informe anual de 2026 vence
/// el 1 de enero, se entregue cuando se entregue.
///
/// Es la clave por la que el planificador decide si una entrega ya existe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FechaNominal(pub NaiveDate);

/// El día contra el que se mide el cumplimiento, ya resuelto el ajuste a hábil.
/// Es un **derivado** del nominal, nunca un sustituto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FechaLimite(pub NaiveDate);

impl FechaNominal {
    pub fn fecha(self) -> NaiveDate {
        self.0
    }
}

impl FechaLimite {
    pub fn fecha(self) -> NaiveDate {
        self.0
    }
}

/// Qué hacer cuando el día nominal no es hábil. **No son tres formas de mover
/// una fecha: son cuatro cosas distintas**, y por eso son un enum y no una
/// bandera booleana como en el motor anterior, donde convivían dos parejas de
/// banderas (`ajuste_habil*` y `mover_habil*`) que ya causaron que el ajuste
/// anticipado no se aplicara nunca en modo semanal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TipoAjuste {
    /// El día nominal es el límite. No hay nada que resolver.
    #[default]
    #[serde(rename = "NINGUNO")]
    Ninguno,
    /// Se entrega el hábil **anterior**. El vencimiento sigue siendo el día
    /// nominal; lo que se adelanta es la entrega, porque el día nominal nadie
    /// trabaja. Es la regla segura: adelantar nunca llega tarde.
    #[serde(rename = "ANTICIPADO")]
    Anticipado,
    /// Período de gracia: se admite entregar hasta el hábil **siguiente**. Lo
    /// que se amplía es el plazo, no el vencimiento.
    #[serde(rename = "GRACIA")]
    Gracia,
    /// El vencimiento **no existe** ese día. Es lo que significa «informe diario
    /// en días laborales»: un festivo no genera un informe que haya que entregar
    /// otro día — genera cero. Tratarlo como los otros produciría el lunes dos
    /// entregas, una por el festivo y otra por el lunes: un incumplimiento
    /// inventado.
    #[serde(rename = "OMITIR")]
    Omitir,
}

/// Un vencimiento: su día nominal, su día límite y qué ajuste los separa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vencimiento {
    pub nominal: FechaNominal,
    pub limite: FechaLimite,
    pub ajuste: TipoAjuste,
    /// Solo en periodicidad por horas. En el resto el vencimiento agota el día.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hora: Option<u32>,
}

impl Vencimiento {
    pub fn nuevo(nominal: NaiveDate, limite: NaiveDate, ajuste: TipoAjuste) -> Self {
        Self { nominal: FechaNominal(nominal), limite: FechaLimite(limite), ajuste, hora: None }
    }

    /// Nominal y límite coinciden: no hubo nada que ajustar.
    pub fn sin_ajuste(nominal: NaiveDate) -> Self {
        Self::nuevo(nominal, nominal, TipoAjuste::Ninguno)
    }

    pub fn con_hora(mut self, hora: u32) -> Self {
        self.hora = Some(hora);
        self
    }

    pub fn se_movio(&self) -> bool {
        self.limite.fecha() != self.nominal.fecha()
    }
}
