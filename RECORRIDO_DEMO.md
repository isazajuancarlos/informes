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
columna de urgencia. Lo que hay, medido en la demostración sembrada: **40
entregas — 22 cumplidas, 5 extemporáneas, 5 incumplidas, 5 vencidas sin registrar
y 2 que vencen esta semana**, con una tasa de cumplimiento del 68 %.

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

La pantalla de cronograma. Se marcan tres días en el calendario —por ejemplo, el
15 de tres meses seguidos— y **el sistema dice qué puede ser**: mensual el día
15, o cada 30 días, o las lecturas que contengan lo señalado, cada una con las
próximas fechas que generaría.

> «No hay que aprenderse una sintaxis de recurrencia. Usted marca lo que sabe y
> el sistema propone; usted elige la lectura correcta y la guarda.»

Se elige una, se guarda, y **las entregas aparecen en la bandeja**. Ese es el
cierre del arco: de una regla a unas tareas con fecha, sin que nadie las escriba.

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
| ¿Cuántos usuarios? | Cuatro papeles: administrador, operador, supervisor y tercero externo. |
| ¿Y si mi norma cambia? | Se edita la regla y las entregas futuras se recalculan. Las pasadas no se tocan: son historia. |

## Lo que NO se promete

- **No es multi-entidad todavía.** Un despliegue sirve a una organización. Si el
  prospecto necesita varias con datos separados, es una conversación distinta y
  hay que tenerla ANTES de instalar: cambiarlo después es una migración.
- **La licencia offline no está portada** al binario nuevo. No se menciona.
- No se cita ninguna resolución ni se da opinión jurídica sobre el valor
  probatorio de la firma. Xiliux vende el sistema; el criterio jurídico es otra
  cosa y va por separado.
