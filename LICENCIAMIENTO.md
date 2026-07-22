# Modelo de licenciamiento y precios — Informes

**Documento interno de decisión comercial. No se entrega al cliente.**
Lo que sí se entrega es el EULA y la cotización, que se derivan de aquí.

Versión 1.1 · 2026-07-21 · Propuesta para revisión de Juan Carlos Isaza
Cifras de mercado ancladas a `ANCLAJE_PRECIOS.md`, reproducibles contra SECOP II.

---

## 1. El modelo, en una frase

**Licencia perpetua sobre la versión entregada + mantenimiento y actualizaciones
anual del 20%**, para los perfiles `onprem` y `standalone`.
**Suscripción mensual** para el perfil `cloud`.

El modelo sigue al perfil de despliegue, no al revés.

---

## 2. Por qué perpetuo en offline (el argumento que manda)

No es una preferencia comercial, es una restricción técnica:

Un cliente air-gapped **no puede revalidar nada contra un servidor**. Si el
modelo fuera suscripción, el archivo de licencia firmado tendría que llevar
fecha de caducidad — y el día que expire, el sistema se apaga solo en un
servidor al que no tenemos acceso, probablemente en una entidad pública,
probablemente en cierre de mes. Ese incidente cuesta el cliente y la referencia.

Con licencia perpetua el archivo firmado **no caduca nunca**: valida
titularidad, no vigencia. El modo de fallo desaparece por diseño, no por
operación cuidadosa.

Ventaja secundaria, real: las entidades públicas colombianas compran **capex**
(inversión + mantenimiento anual) mucho mejor de lo que compran opex recurrente
indefinido. El modelo encaja con cómo presupuestan, no pelea contra ello.

---

## 3. Las cinco reglas que no se negocian

### 3.1. Se llama «mantenimiento y actualizaciones», nunca «soporte»

`Informes` es un producto **normativo**: festivos, plazos y requisitos de
reporte cambian. El software se degrada solo si nadie lo actualiza. Por eso la
cuota anual tiene valor verificable y no es un peaje por contestar correos.

Además sobrevive a los recortes: «soporte» es la primera línea que un comité de
compras tacha; «actualizaciones normativas» no.

### 3.2. Escribir explícitamente qué es perpetuo

Es la ambigüedad que genera pleitos. La redacción tiene que decir:

> La licencia es perpetua **sobre la versión entregada**. Mientras el
> mantenimiento esté vigente, el cliente tiene derecho a todas las versiones
> nuevas. Si lo deja vencer, **conserva la última versión recibida y sigue
> operando indefinidamente**, sin derecho a versiones posteriores.

Sin esa frase, el cliente cree que compró actualizaciones para siempre.

### 3.3. Primer año de mantenimiento incluido y obligatorio

Va dentro del precio inicial. Así el año 1 nunca es «solo licencia», y el
cliente entra al ciclo de renovación por defecto en vez de por decisión.

### 3.4. Cuota de reactivación si dejan vencer

**50% del valor de la licencia vigente**, más las anualidades no pagadas.
Sin esto, el cliente salta tres años y renueva solo el mes que necesita algo.

### 3.5. El mantenimiento se cotiza sobre el precio de lista vigente

No sobre el precio con descuento que pagó el cliente. Si no, un descuento de
lanzamiento del 40% se convierte en un descuento perpetuo sobre la renta.

---

## 4. Cómo se valora, y con qué se compara

Hay precios de referencia —§4.4— pero antes va el método, porque una lista
enumera tres casos y un método sirve para cualquier entidad.

Lo que sigue no es negociable con uno mismo: es la baranda que evita que la
tercera reunión dura acabe regalando el trabajo.

> **Corrección del 2026-07-21.** La versión anterior de este apartado afirmaba
> que los precios estaban «bajos por un orden de magnitud» y retiraba la lista
> por eso. **Esa afirmación era falsa.** Salía de pedirle a la API los 200
> contratos de mayor valor y sacar la mediana de ahí: cuando una muestra se
> ordena por la variable que se va a medir, deja de ser una muestra. Subir
> precios sobre esa medición habría dejado el producto fuera de mercado sin que
> nadie supiera por qué. Todo lo de aquí abajo sale ahora del universo completo
> y es reproducible con `python analisis_precios_secop.py`; el detalle y los
> límites están en [`ANCLAJE_PRECIOS.md`](ANCLAJE_PRECIOS.md).

### 4.1. El anclaje: qué gasta hoy la entidad en el mismo problema

**Antes de cotizar, se consulta SECOP.** Los contratos de las entidades públicas
son públicos: se busca qué paga hoy la entidad por hacer a mano lo que el sistema
hace, y esa cifra —no nuestro costo— es el punto de partida de la conversación.

**Software de seguimiento** — 2.624 contratos, el comparable directo:

| p25 | mediana | p75 | p90 |
|---|---|---|---|
| $18,1M | **$36,5M** | $66,1M | $118,0M |

**Una persona haciendo el trabajo a mano** — 3.344 contratos, anualizados. Es el
competidor real, y la anualización no es un detalle: la duración mediana es de
180 días, así que comparar sin normalizar subestimaría el costo anual a la mitad.

| p25 | mediana | p75 | p90 |
|---|---|---|---|
| $42,6M | **$54,8M / año** | $73,6M | $99,2M |

Los contratos de $100M–$150M que citaba la versión anterior existen —JEP, ANH,
IDU, UBPD— pero son el percentil 90 y superior. Sirven como ejemplo de que el
gasto es real y recurrente; **no como mediana**.

**El argumento, con las cifras corregidas:** una entidad que hoy contrata a una
persona para esta función gasta la mediana de $54,8M **todos los años**. El
sistema se paga una vez.

### 4.2. Por dónde entra la venta

| Modalidad | Cuota |
|---|---|
| **Contratación directa** | **85,4%** |
| Régimen especial | 5,3% |
| Mínima cuantía | 3,2% |

**El 88% del mercado entra por contratación directa o mínima cuantía**, y el
artículo 6 de la Ley 1150 de 2007 **exime a ambas del RUP**. El temor de que
«experiencia acreditada en contratos similares» dejara fuera al primer cliente no
se sostiene: por la puerta que usa el 85% de este mercado, ese requisito no es el
filtro. Solo 10 de 2.646 contratos fueron por licitación pública.

### 4.3. Qué determina el número en cada caso

En orden de peso:

1. **Cuánto gasta hoy la entidad** en la función (§4.1). Es el techo razonable.
2. **Cuántas obligaciones de reporte** administra: es lo que escala el valor,
   no el número de usuarios.
3. **Si el reporte tiene fundamento legal** — como el SGR de la Ley 2056 — el
   incumplimiento tiene consecuencia disciplinaria, y eso cambia la conversación
   entera. No se vende miedo; se constata quién responde si el informe no sale.
4. **Perfil de despliegue.** `standalone` es instalación asistida y poco más;
   `onprem` air-gapped implica traslado, kit offline y acompañamiento en sitio.
5. **Recurrencia.** Si ya contratan a alguien todos los años, se compara contra
   el acumulado de tres años, no contra el de uno.

### 4.4. Precios de referencia, y dónde caen en el mercado

**Cifras en COP, antes de IVA.** No son una tarifa que se publique en una web:
son el punto desde el que se aplica §4.3. Un precio publicado es un ancla desde
la que el comprador negocia hacia abajo.

| Edición | Perfil | Alcance | Licencia perpetua | Mantenimiento (20%) |
|---|---|---|---|---|
| **Esencial** | `standalone` | Hasta 25 usuarios · 1 sede · SQLite | **$12.000.000** | $2.400.000 |
| **Profesional** | `onprem` | Hasta 100 usuarios · multi-área · PostgreSQL | **$28.000.000** | $5.600.000 |
| **Corporativa** | `onprem` | Usuarios ilimitados · multi-sede · air-gapped | **$55.000.000** | $11.000.000 |

**Año 1 = licencia + mantenimiento.** La Profesional se factura en $33.600.000 el
primer año y $5.600.000 cada año siguiente.

Contra la medición de §4.1:

| Edición | Percentil del software comparable | Equivale a |
|---|---|---|
| Esencial | 12 | **2,6 meses** de una persona |
| Profesional | 40 | **6,1 meses** de una persona |
| Corporativa | 68 | **12,1 meses** de una persona |

**La Corporativa perpetua cuesta lo mismo que UN AÑO de UNA persona**, y la
persona vuelve a costar lo mismo al año siguiente. Ese es el argumento, y es una
cifra medida.

Lo que sí conviene revisar es la **Esencial a $12M**: en el percentil 12 compite
con mínima cuantía, donde el margen es estrecho y el cliente pide más soporte por
peso. Es la misma razón por la que en Quipu OPRF se retiró el tramo de 9 USD.

### 4.5. Las barandas, que sí son fijas

| Regla | Valor | Por qué |
|---|---|---|
| Mantenimiento anual | **20% del precio de lista** | Rango de mercado 18–22%; por debajo de 15% trabajas gratis |
| Primer año de mantenimiento | **Incluido y obligatorio** | El año 1 nunca es solo licencia |
| Reactivación tras vencer | **50% de la licencia** + anualidades no pagadas | Si no, saltan tres años y renuevan solo cuando necesitan algo |
| Descuento máximo acumulado | **35%** | Por debajo, el proyecto no paga la implementación |
| Piso absoluto de licencia | **El costo de implementación × 2** | Nunca se firma por debajo. Si no lo cubre, no es cliente |

**Los porcentajes son la protección real**, no las cifras: se mantienen sea cual
sea el valor negociado. Los precios de §4.4 se negocian; estas barandas, no.

### 4.6. Servicios, siempre aparte

Implementación y parametrización, migración de datos, capacitación, desarrollo a
la medida e instalación air-gapped asistida **se cotizan por separado y no se
regalan**. Es donde se va el tiempo real y es lo que un cliente entiende que se
paga. Se valoran por horas efectivas contra la tarifa vigente, no por porcentaje
de la licencia.

### 4.7. Suscripción `cloud`

Se calibra para que **se cruce con la compra perpetua alrededor del año 3**:
antes el cloud sale más barato, después el perpetuo. Esa es la conversación
honesta, y además empuja a las entidades grandes hacia el perpetuo, que es donde
el producto encaja mejor.

---

## 5. Cómo se defienden los números

No son un múltiplo arbitrario; hay un ancla que se puede decir en voz alta:

Un analista de cumplimiento en Colombia cuesta del orden de **$4.500.000
mensuales cargados** (~$54.000.000 al año). Si `Informes` le ahorra el 40% del
tiempo a una sola persona, libera ~**$21.600.000 al año**. La edición
Profesional a $28.000.000 **se paga en menos de 18 meses** y a partir de ahí
solo cuesta $5.600.000 anuales.

Ese es el pitch. Es verificable, el cliente puede hacer la cuenta con sus
propios sueldos, y no depende de convencerlo de nada abstracto.

El otro ancla, para entidades públicas: **el costo de un hallazgo**. Un informe
entregado fuera de plazo puede escalar a un proceso disciplinario. Ahí el
producto no se compara contra un sueldo sino contra un riesgo, y el precio deja
de ser el eje de la conversación. Usar este ángulo con cuidado y sin exagerar:
se vende trazabilidad, no miedo.

---

## 6. Descuentos, con techo

| Situación | Descuento máximo |
|---|---|
| Cliente de referencia (autoriza caso de éxito con nombre) | 25% |
| Pago anticipado del 100% a la firma | 10% |
| Segunda entidad del mismo grupo | 15% |
| Renovación multianual de mantenimiento (3 años) | 10% |

**No acumulables por encima del 35%.** Por debajo de ese piso el proyecto no
paga la implementación.

---

## 7. Propiedad intelectual y Quipu

- `Informes` es **software propietario**, todos los derechos reservados.
  Corresponde crear `LICENSE` con esa declaración expresa; hoy el repositorio no
  tiene ninguna, y el silencio protege pero no sirve para vender.
- La firma post-cuántica de entregas usa **`quipu-crypto`, que es AGPL-3.0**.
  Verificado el 2026-07-18 con `git shortlog -sne --all` sobre el repositorio de
  Quipu: los únicos autores son Juan Carlos Isaza y dependabot. **Titular único
  ⇒ el copyleft de la AGPL no alcanza a `Informes`**, porque uno no se infecta a
  sí mismo: se autolicencia bajo sus propios términos comerciales.
- **Condición para que eso siga siendo cierto:** si algún día se acepta un PR
  externo en Quipu, hace falta **CLA o DCO con cesión de derechos antes de
  mergearlo**. Sin eso, ese fragmento queda AGPL de un tercero y bloquea el uso
  propietario para siempre.
- Falta un `THIRD_PARTY.md` con las licencias de las dependencias. La única de
  la lista que exige nota expresa es **`psycopg2-binary` (LGPL con excepción)** —
  compatible con producto propietario en Python porque se importa, no se enlaza
  estáticamente, pero hay que declararla.

---

## 8. Validación de licencia offline (pendiente técnico)

El perpetuo simplifica el diseño, pero no lo elimina. El plan:

1. Archivo `licencia.qlic` firmado con **ML-DSA-87 vía Quipu**, ya prototipado
   en `app/services/signing_service.py`.
2. Contiene: razón social, NIT, edición, límite de usuarios, fecha de emisión,
   **sin fecha de caducidad**.
3. La app verifica la firma **en local, al arrancar**, contra la clave pública
   embebida. Sin red.
4. Si falta o es inválida: la app **no se apaga** — arranca en modo restringido
   y avisa. Nunca dejar a un cliente sin operación por un problema de licencia.
5. El límite de usuarios se comprueba al crear el usuario nº N+1, con un aviso
   claro, no con un bloqueo silencioso.

La regla de fondo: **la licencia se hace cumplir por contrato, no rompiendo el
software del cliente.**

---

## 9. Pendientes que se derivan de este documento

- [ ] `LICENSE` propietario en la raíz del repositorio
- [ ] EULA / contrato de licencia de uso (redactado sobre este modelo)
- [ ] `THIRD_PARTY.md` con licencias de dependencias
- [ ] Plantilla de cotización con las tres ediciones
- [ ] Implementar `licencia.qlic` (§8)
- [ ] CLA/DCO en Quipu antes de aceptar el primer PR externo
