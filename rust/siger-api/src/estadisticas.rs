//! Estadísticas de cumplimiento, SIEMPRE dentro del alcance del actor.
//!
//! No hay un parámetro que el cliente pueda mandar para ver más de lo que le
//! toca: el `WHERE` sale de [`crate::autz::Alcance`], igual que en la lista de
//! entregas. Un operador ve sus números; un supervisor o un admin, todos; un
//! tercero, los del stakeholder que representa. La misma página sirve a todos
//! los roles porque el backend ya recorta.

use axum::{extract::State, Json};
use serde::Serialize;

use crate::autz::{Actor, Alcance};
use crate::rutas::{Error, Estado};

/// El fragmento `WHERE` del alcance y su parámetro, para reutilizarlo en cada
/// agregación. `None` en el parámetro = sin bind. La cláusula ya incluye el
/// `WHERE`, y se le concatena el resto con `AND`.
fn alcance_sql(a: &Alcance) -> (&'static str, Option<i64>) {
    match a {
        Alcance::Todas => ("WHERE 1", None),
        Alcance::DeUsuario(id) => ("WHERE e.usuario_id = ?", Some(*id)),
        Alcance::DeStakeholder(id) => (
            "WHERE EXISTS (SELECT 1 FROM participacion p \
             WHERE p.informe_id = e.informe_id AND p.stakeholder_id = ?)",
            Some(*id),
        ),
        Alcance::Ninguna => ("WHERE 0", None),
    }
}

#[derive(Serialize, Default)]
pub struct Estadisticas {
    pub(crate) total: i64,
    pub(crate) pendientes: i64,
    pub(crate) cumplidas: i64,
    pub(crate) extemporaneas: i64,
    pub(crate) incumplidas: i64,
    /// Pendientes cuyo límite cae dentro de los próximos 7 días.
    pub(crate) proximas_7d: i64,
    /// Pendientes cuyo límite ya pasó (vencidas sin registrar).
    pub(crate) vencidas: i64,
    /// Cumplidas / (total menos las que aún están pendientes), en 0..=100. Se
    /// mide sobre lo que YA venció y debía estar entregado, no sobre el total
    /// —incluir lo que todavía no vence castigaría un número que no es culpa de
    /// nadie—. `None` si no hay nada vencido todavía: no se inventa un 100%.
    pub(crate) tasa_cumplimiento: Option<i64>,
}

/// El resumen que le corresponde a este actor. Aparte del manejador porque lo
/// usan `GET /api/estadisticas` y la pantalla del panel, y el `alcance_sql` es
/// justo lo que no puede tener dos versiones.
pub(crate) async fn resumen_de(e: &Estado, actor: &Actor) -> Result<Estadisticas, Error> {
    let (scope, param) = alcance_sql(&actor.alcance());
    let hoy = chrono::Utc::now().date_naive().to_string();
    let tope = (chrono::Utc::now().date_naive() + chrono::Duration::days(7)).to_string();

    // Conteo por estado, en una sola consulta agrupada dentro del alcance.
    let filas = {
        let sql = format!("SELECT e.estado, COUNT(*) AS n FROM entregas e {scope} GROUP BY e.estado");
        let mut q = sqlx::query_as::<_, (String, i64)>(&sql);
        if let Some(v) = param {
            q = q.bind(v);
        }
        q.fetch_all(&e.bd).await?
    };

    let mut st = Estadisticas::default();
    for (estado, n) in filas {
        match estado.as_str() {
            "PENDIENTE" => st.pendientes = n,
            "CUMPLIDO" => st.cumplidas = n,
            "EXTEMPORANEO" => st.extemporaneas = n,
            "INCUMPLIDO" => st.incumplidas = n,
            _ => {}
        }
        st.total += n;
    }

    // Próximas y vencidas: pendientes con el límite cerca o pasado.
    let prox_sql = format!(
        "SELECT COUNT(*) FROM entregas e {scope} AND e.estado = 'PENDIENTE' \
         AND e.fecha_limite >= ? AND e.fecha_limite <= ?"
    );
    let venc_sql = format!(
        "SELECT COUNT(*) FROM entregas e {scope} AND e.estado = 'PENDIENTE' AND e.fecha_limite < ?"
    );
    // `WHERE 1`/`WHERE 0` permiten el `AND` siguiente sin ramificar el SQL.
    let mut qp = sqlx::query_scalar::<_, i64>(&prox_sql);
    if let Some(v) = param {
        qp = qp.bind(v);
    }
    st.proximas_7d = qp.bind(&hoy).bind(&tope).fetch_one(&e.bd).await?;

    let mut qv = sqlx::query_scalar::<_, i64>(&venc_sql);
    if let Some(v) = param {
        qv = qv.bind(v);
    }
    st.vencidas = qv.bind(&hoy).fetch_one(&e.bd).await?;

    // Tasa sobre lo ya resuelto (cumplidas + extemporáneas + incumplidas), no
    // sobre el total: lo que aún no vence no cuenta ni a favor ni en contra.
    let resueltas = st.cumplidas + st.extemporaneas + st.incumplidas;
    st.tasa_cumplimiento = if resueltas > 0 {
        Some((st.cumplidas * 100) / resueltas)
    } else {
        None
    };

    Ok(st)
}

pub async fn resumen(State(e): State<Estado>, actor: Actor) -> Result<Json<Estadisticas>, Error> {
    Ok(Json(resumen_de(&e, &actor).await?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_alcance_recorta_y_ninguna_no_devuelve_nada() {
        assert_eq!(alcance_sql(&Alcance::Todas).0, "WHERE 1");
        assert_eq!(alcance_sql(&Alcance::DeUsuario(7)), ("WHERE e.usuario_id = ?", Some(7)));
        assert_eq!(alcance_sql(&Alcance::Ninguna).0, "WHERE 0");
        assert_eq!(alcance_sql(&Alcance::DeStakeholder(3)).1, Some(3));
    }
}
