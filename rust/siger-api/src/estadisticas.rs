//! Estadísticas de cumplimiento, SIEMPRE dentro del alcance del actor.
//!
//! No hay un parámetro que el cliente pueda mandar para ver más de lo que le
//! toca: el `WHERE` sale del motor (`crate::autz::AlcanceEfectivo`), igual que en la lista de
//! entregas. Un operador ve sus números; un supervisor o un admin, todos; un
//! tercero, los del stakeholder que representa. La misma página sirve a todos
//! los roles porque el backend ya recorta.

use axum::{extract::State, Json};
use serde::Serialize;

use crate::autz::Actor;
use crate::rutas::{Error, Estado};

/// El fragmento `WHERE` del alcance y su parámetro, para reutilizarlo en cada
/// agregación. `None` en el parámetro = sin bind. La cláusula ya incluye el
/// `WHERE`, y se le concatena el resto con `AND`.
fn alcance_sql(actor: &Actor) -> (String, Vec<i64>) {
    use crate::autz::{Accion, Predicado};
    // `WHERE 1`/`WHERE 0` (y `WHERE (predicado)`) permiten el `AND` siguiente sin ramificar
    // el SQL. El predicado sale del motor; alias `e.`, el informe por `e.informe_id`.
    match actor.predicado(Accion::VerEntregas, "e.", "e.informe_id") {
        Predicado::SinFiltro => ("WHERE 1".to_string(), Vec::new()),
        // Entre paréntesis: a este scope se le concatena ` AND estado… AND fecha…`, así que
        // un predicado multi-ámbito (`a OR b`, lo que la fusión habilitará) sin envolver se
        // parsearía `a OR (b AND …)` por precedencia — un conteo incorrecto. Con 1 ámbito
        // (hoy) da igual; se parentiza YA para que el motor y este consumidor no diverjan.
        Predicado::Filtro { sql, binds } => (format!("WHERE ({sql})"), binds),
        Predicado::Nada => ("WHERE 0".to_string(), Vec::new()),
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
    let (scope, binds) = alcance_sql(actor);
    let hoy = chrono::Utc::now().date_naive().to_string();
    let tope = (chrono::Utc::now().date_naive() + chrono::Duration::days(7)).to_string();

    // Conteo por estado, en una sola consulta agrupada dentro del alcance.
    let filas = {
        let sql = format!("SELECT e.estado, COUNT(*) AS n FROM entregas e {scope} GROUP BY e.estado");
        let mut q = sqlx::query_as::<_, (String, i64)>(&sql);
        for b in &binds {
            q = q.bind(*b);
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
    for b in &binds {
        qp = qp.bind(*b);
    }
    st.proximas_7d = qp.bind(&hoy).bind(&tope).fetch_one(&e.bd).await?;

    let mut qv = sqlx::query_scalar::<_, i64>(&venc_sql);
    for b in &binds {
        qv = qv.bind(*b);
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
        use crate::autz::Rol;
        let actor = |rol, sk| Actor {
            id: 7,
            nombre: "p".into(),
            email: "p@c.co".into(),
            rol,
            stakeholder_id: sk,
        };
        assert_eq!(alcance_sql(&actor(Rol::Admin, None)).0, "WHERE 1");
        let (sql, binds) = alcance_sql(&actor(Rol::Operador, None));
        assert_eq!(sql, "WHERE (e.usuario_id = ?)");
        assert_eq!(binds, vec![7]);
        assert_eq!(alcance_sql(&actor(Rol::Tercero, None)).0, "WHERE 0");
        assert_eq!(alcance_sql(&actor(Rol::Tercero, Some(3))).1, vec![3]);
    }
}
