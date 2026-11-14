# Los juegos de meses fijos: qué dejó fuera la pantalla vieja

Medido el 2026-07-30 contra `informes_local.db`, la base migrada real
(276 informes del catálogo del cliente). Responde el tercer cabo de la tarea
#213, que preguntaba si el desplegable de periodicidad estuvo dejando
obligaciones mal registradas.

**La respuesta es sí, y son 61.**

## Qué fijaba la pantalla vieja

Verificado en la historia de git, no citado de memoria
(`git show e0c8a98^:angular/src/app/paginas/cronograma.ts`, línea 48):

```ts
const MESES_DE: Record<Periodicidad, number[]> = {
  mensual:    [1,2,3,4,5,6,7,8,9,10,11,12],
  trimestral: [3, 6, 9, 12],
  semestral:  [6, 12],
  anual:      [12],
  semanal:    [],
};
```

Cuatro juegos, y **al guardar sobrescribía `meses` con el del desplegable**
(línea 330: `const meses = MESES_DE[this.periodicidad()]`). Es decir, no solo no
se podían crear otros: abrir uno que ya los tuviera y guardar **lo reescribía**.

## Lo que hay en la base real

| | informes |
|---|---|
| ESPECIFICO | 185 |
| SEMANAL | 71 |
| INMEDIATO (a solicitud) | 16 |
| CICLICO | 4 |
| **total** | **276** |

De los 185 específicos:

| ¿lo expresaba el desplegable? | informes |
|---|---|
| sí — mensual `[1..12]` | 124 |
| **no** | **61** |

Ninguno de los 61 encaja en `trimestral`, `semestral` ni `anual` tal como los
fijaba la pantalla. Los patrones que aparecen:

- **`[1, 4, 7, 10]`** — trimestre desplazado, el más común con diferencia.
  Enero-abril-julio-octubre es un trimestre perfectamente normal; el desplegable
  solo sabía marzo-junio-septiembre-diciembre.
- **`[1, 7]`** — semestral desplazado (enero y julio, no junio y diciembre).
- **`[1]`** — anual que vence en enero. El desplegable solo sabía diciembre.

## Por qué importa

Un informe con `[1,4,7,10]` abierto en la pantalla vieja y guardado quedaba con
`[3,6,9,12]`: **la obligación se movía dos meses** y nadie recibía un aviso. No
es una limitación de la interfaz, es una vía de corrupción silenciosa de datos —
y afectaba a un tercio de los informes específicos del catálogo.

El cronograma por inferencia (commit `e0c8a98`) la cierra por construcción: ya no
hay un desplegable que imponga un juego de meses. Se marcan los días, el sistema
propone las lecturas que **contienen** lo marcado, y la regla que se guarda es la
que se enseñó.

## Cómo repetir la medición

```bash
python3 - <<'PY'
import sqlite3, json, collections
c = sqlite3.connect('informes_local.db')
JUEGOS = {tuple(range(1,13)):'mensual', (3,6,9,12):'trimestral',
          (6,12):'semestral', (12,):'anual'}
fuera = []
for iid, nombre, rec in c.execute('SELECT id, nombre, recurrencia_json FROM cat_informes'):
    r = json.loads(rec) if rec else {}
    if (r.get('modo') or '').upper() != 'ESPECIFICO':
        continue
    meses = tuple(sorted(int(x) for x in (r.get('meses') or [])))
    if meses not in JUEGOS:
        fuera.append((iid, nombre, meses))
print(len(fuera), 'informes con meses que el desplegable no expresaba')
PY
```
