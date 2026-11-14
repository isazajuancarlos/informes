# El recorrido de cinco minutos

Guion para enseñar SIGER a un prospecto. **No es un manual**: es el orden en que
se enseña, con lo que hay que decir en cada pantalla y —más importante— lo que
NO hay que decir.

La regla de fondo: lo que convence no es la lista de funciones, es **ver su
obligación con su plazo y su semáforo**. Todo lo que no sea eso alarga la
demostración y la debilita.

Credenciales de la demostración: `admin@co.co` / `admin`,
`operador@co.co` / `operador`, `tercero@co.co` / `tercero`.

---

## Antes de empezar (30 segundos, sin pantalla)

Una sola frase para fijar el problema, y callarse:

> «Usted tiene obligaciones de reporte con fechas. Hoy viven en una hoja de
> cálculo que alguien mantiene, y el día que esa persona no está, nadie sabe qué
> se venció.»

Si asiente, el resto de la demostración tiene sentido. Si dice que no —que ya
tiene un sistema—, la pregunta que sigue es **cuál es y qué le falta**, no seguir
con el recorrido.

## 1. La bandeja (1 minuto) — entrar como `operador`

Es la primera pantalla y hace el trabajo sola: entregas con su fecha límite y la
columna de urgencia. Lo que hay, en la siembra del **2026-08-10**: **7 informes
que cubren los cinco modos del motor**, con **21 cumplidas, 5 extemporáneas, 5
incumplidas y 5 vencidas sin registrar**, y el resto pendientes por vencer.

> **Esas cifras son de UNA siembra, y las fechas de la demo son RELATIVAS al día
> en que se sembró** («vencida hace tres días», «vence mañana» — está escrito en
> la cabecera de `sembrar_demo.py`). Cuanto más vieja la siembra, más cosas
> aparecen vencidas y menos dice la columna de urgencia. **Antes de enseñarla,
> resiembra** —`./despliegue/desplegar-demo.sh` lo hace— y toma los conteos
> reales de lo que imprima el guion al terminar, en vez de leer estos en voz
> alta.

Las **vencidas sin registrar** son las que hacen la conversación: nadie las tocó
y el plazo pasó. Es el caso que el prospecto reconoce sin que se lo expliquen.

**Qué señalar**: que las fechas se calcularon solas desde una regla, y que la
urgencia no es un color decorativo sino el resultado de comparar dos fechas — la
nominal y la límite, que no siempre son la misma.

**Qué NO hacer**: recorrer el menú. Nadie compra un menú.

## 2. Las dos fechas (1 minuto) — abrir una entrega con ajuste

Abrir una que tenga tipo de ajuste `ANTICIPADO` o `GRACIA`.

> «El informe vence el último día del trimestre. Si ese día es domingo, ¿usted
> entrega el lunes o el viernes? Su norma dice una de las dos, y el sistema lo
> sabe: la fecha nominal es la de la obligación y la límite es la que usted debe
> cumplir.»

Esto es lo que ninguna hoja de cálculo hace, y es el momento en que el prospecto
entiende que hay un motor debajo. **El calendario de festivos va dentro**: 484
festivos día a día de 2020 a 2045, contrastados contra 8 534 vencimientos reales.

## 3. Crear una obligación marcando días (1,5 minutos) — como `admin`

La pantalla `/cronograma`, bloque **«¿Qué regla explica estas fechas?»**. Se
marcan tres días en el calendario —por ejemplo el 15 de tres meses seguidos—, se
marca el ajuste si la norma lo pide (hasta el hábil siguiente, el hábil anterior,
o sin vencimiento en festivo) y se pulsa **«Ver lecturas posibles»**. **El
sistema dice qué puede ser**: mensual el día 15, o cada 30 días, o las lecturas
que contengan lo señalado, **cada una con las próximas fechas que generaría**.

> «No hay que aprenderse una sintaxis de recurrencia. Usted marca los días que
> sabe y el sistema propone; usted elige la lectura correcta y la guarda.»

El campo de texto sigue debajo y sigue funcionando: quien prefiera teclear las
fechas —o pegarlas de un correo— puede hacerlo, y el calendario le sigue. **No
es un adorno sobre el campo: el campo es la fuente de verdad y los dos sentidos
están probados.**

Se elige una, se copia su JSON al informe (`/informes` → «Crear informe» →
*Regla de periodicidad*), se vuelve a `/cronograma` y se pulsa **«Generar
ahora»**: **las entregas aparecen en la bandeja**. Ese es el cierre del arco: de
una regla a unas tareas con fecha, sin que nadie las escriba.

> **HISTORIA DE ESTE PÁRRAFO, porque explica qué se puede prometer.** El
> calendario era un componente de Angular; al pasar el panel a `maud` + htmx
> (2026-08-06) no se portó, y durante cuatro días las fechas solo se escribían
> —la función intacta, el gesto perdido—. Volvió el **2026-08-10**, y **no en
> este producto**: vive en `boi`, la micro-librería compartida del estándar, así
> que lo estrenan aquí y lo heredan los demás frontales. Si alguien pregunta por
> el frontal, ése es el argumento: no se reescribe cinco veces.

## 4. La entrega y su rastro (1 minuto)

Subir un archivo a una entrega pendiente. Medido: responde 200 y la entrega pasa
a `CUMPLIDO` con el archivo renombrado por el servidor
(`entrega-17-20260730-174703.pdf`) — ninguna cadena que venga del navegador se
usa como ruta. Después, la auditoría:

- el archivo se guarda **cifrado en reposo** (Argon2 + AEAD) si el despliegue
  tiene `SIGER_CLAVE_REPOSO`;
- la entrega se **firma** con criptografía post-cuántica cuando hay claves;
- todo queda en una **bitácora**.

**Los dos condicionales están resueltos en la demostración desplegada**:
`desplegar-demo.sh` genera las claves de firma y la frase de cifrado, así que lo
que se enseña es lo que el producto hace. Medido con las claves puestas: la
entrega sube con `tiene_firma: true`, el archivo en disco empieza por la cabecera
`QUIP` —está cifrado, no en claro— y la descarga lo devuelve legible.

**Si alguna vez enseña una demo local sin esas variables**, la entrega sale con
`tiene_firma: false` y el archivo en claro. Decir «está firmado» ahí es una
afirmación que la propia pantalla desmiente delante del cliente.

**Y sin prometer de más**: la firma acredita el contenido del documento en el
momento en que se entregó. **No** diga «tiene valor probatorio pleno» ni cite una
norma: eso es una afirmación jurídica y depende del caso.

## 5. El tercero (30 segundos) — entrar como `tercero`

Medido en la demostración sembrada: ve **8 entregas de 3 informes** —los dos de
los que es destino y aquel del que es origen—, no las 40 de los siete. Y el
catálogo de informes le responde **«no autorizado»**: no es que vea una lista
recortada, es que esa pantalla no es suya.

No es una interfaz distinta: es el mismo sistema con el alcance de quien mira.

> «La entidad a la que usted reporta puede entrar y ver lo suyo, sin ver lo de
> nadie más y sin que usted le mande un correo.»

---

## Lo que hay que decir cuando lo pregunten

| Pregunta | Respuesta honesta |
|---|---|
| ¿Está en la nube o en mi servidor? | Las dos. Es **un archivo ejecutable** con la base dentro; corre en un servidor suyo sin internet, o se lo alojamos. |
| ¿Qué pasa si se me daña? | `siger-api respaldar` copia la base **en caliente** con sus archivos y **verifica la copia**. La restauración está probada, no supuesta. |
| ¿Manda correos? | Sí, avisos de proximidad («Recordatorio:») e incumplimiento («Vencido:»). Probado de punta a punta: 7 mensajes sobre los datos de la demostración. Están **dormidos** hasta que se configura el servidor de correo, y el sistema no arranca con esa configuración a medias. |
| ¿Cuántos usuarios? | Cuatro papeles: administrador, operador, supervisor y tercero externo. El administrador los crea, edita y desactiva desde `/cuentas`, sin tocar la base. |
| ¿Y la seguridad de la entrada? | Contraseña con Argon2id, **segundo factor TOTP** opcional por cuenta —con diez códigos de recuperación en papel— y freno al ensayo de contraseñas que cuenta por cuenta **y** por origen, sin bloquear nunca al usuario legítimo. |
| ¿Y si mi norma cambia? | Se edita la regla y las entregas futuras se recalculan. Las pasadas no se tocan: son historia. |

## Lo que NO se promete

- **No es multi-entidad todavía.** Un despliegue sirve a una organización. Si el
  prospecto necesita varias con datos separados, es una conversación distinta y
  hay que tenerla ANTES de instalar: cambiarlo después es una migración.
- **No es multi-entidad** (arriba). Lo que sí está, y este guion no enseña por
  falta de minutos: **licencia offline** verificada sin red (`licencia.rs`,
  `GET /api/licencia`) y **segundo factor TOTP** con códigos de recuperación
  (`/dos-pasos`). La línea que decía «la licencia offline no está portada» era de
  julio y **ya no es cierta**: se corrigió el 2026-08-10 al verificar la ficha
  #605. Si el prospecto pregunta por seguridad o por licenciamiento, existen —
  pero enseñarlos alarga la demostración y la debilita.
- No se cita ninguna resolución ni se da opinión jurídica sobre el valor
  probatorio de la firma. Xiliux vende el sistema; el criterio jurídico es otra
  cosa y va por separado.
