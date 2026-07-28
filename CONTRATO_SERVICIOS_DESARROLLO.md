# CONTRATO DE PRESTACIÓN DE SERVICIOS DE DESARROLLO DE SOFTWARE — VÍA A

> **Borrador de trabajo (Xiliux / Isaza Arenas).** Es la minuta de la **Vía A**
> del modelo de dos vías (ver `LICENCIAMIENTO.md §10` y tarea #42): se presta un
> servicio de desarrollo que **deriva en un producto con soporte**. NO es la
> licencia de un producto ya hecho (esa es la Vía B, `EULA_BORRADOR.md`).
>
> La cláusula que decide todo es la **OCTAVA — Propiedad intelectual**: sin ella
> bien escrita, la Ley 1450/2011 presume que el desarrollo se transfiere al
> cliente y **se rompe el volante A→B**. Léela primero.
>
> Las cifras y opciones marcadas `[ ]` son de decisión de Juan (abogado) al
> instanciar el contrato para cada cliente.

---

## COMPARECIENTES

**EL CONTRATISTA:** Juan Carlos Isaza Arenas, mayor de edad, identificado con
C.C. No. [___], quien actúa en nombre propio bajo la marca **Xiliux**, con
domicilio en Medellín, Colombia.

**EL CONTRATANTE:** [Razón social], NIT [___], representada por [___], con
domicilio en [___].

En adelante, conjuntamente, **LAS PARTES**.

## CONSIDERACIONES

1. Que EL CONTRATISTA se dedica al desarrollo de software y es **titular de un
   conjunto de herramientas, librerías, marcos de trabajo y productos
   preexistentes** (en adelante, los **«Activos Preexistentes»**), entre ellos
   —sin limitarse a— sus marcos en Rust/Angular, el motor de recurrencia, la
   librería de criptografía post-cuántica *Quipu* y sus productos de catálogo.
2. Que EL CONTRATANTE requiere el desarrollo de una solución de software que **no
   forma parte del catálogo** de EL CONTRATISTA, descrita en el **Anexo 1**.
3. Que LAS PARTES acuerdan regirse por la Ley 23 de 1982 (modificada por la Ley
   1450 de 2011 y la Ley 1915 de 2018), la Decisión Andina 351, la Ley 1581 de
   2012 y demás normas aplicables.

# CLÁUSULAS

## PRIMERA — OBJETO

EL CONTRATISTA se obliga a desarrollar, de manera **independiente y con plena
autonomía técnica, administrativa y financiera**, la solución de software
descrita en el **Anexo 1 (Requerimientos)**, y a entregarla conforme al **Anexo 2
(Condiciones económicas y cronograma)**. El alcance del contrato es, exacta y
únicamente, el conjunto de requerimientos del Anexo 1 con la complejidad allí
clasificada; todo lo demás se rige por la Cláusula CUARTA (Control de cambios).

## SEGUNDA — ALCANCE, REQUERIMIENTOS Y COMPLEJIDAD

2.1. El objeto se descompone en **requerimientos**, cada uno clasificado por
complejidad —**Leve, Moderado, Grave o Crítico**— en el Anexo 1. Esa
clasificación, pactada por escrito **antes** de iniciar, determina el esfuerzo y
el precio (Anexo 2) y **fija el límite del contrato**.

2.2. **Lo que no está en el Anexo 1 no está en el contrato.** Ningún requerimiento
adicional, ni la profundización de uno existente más allá de su complejidad
pactada, se entiende incluido.

2.3. **Revisiones incluidas.** Cada requerimiento incluye **[dos (2)]** rondas de
ajuste sobre lo entregado. A partir de la siguiente, el ajuste se cotiza por
horas a la tarifa vigente.

2.4. **Tope por fase.** El desarrollo se ejecuta por fases de máximo **[N]**
requerimientos; cada fase se cierra, se recibe a satisfacción y se paga antes de
iniciar la siguiente.

## TERCERA — RECEPCIÓN Y ACEPTACIÓN

Cada entregable se somete a **pruebas de aceptación** acordadas en el Anexo 1. EL
CONTRATANTE dispone de **[cinco (5)] días hábiles** para aprobar u observar por
escrito; el silencio transcurrido ese plazo se entiende como **aceptación**. Las
observaciones deben referirse a incumplimientos del requerimiento pactado, no a
requerimientos nuevos (que van por la Cláusula CUARTA).

## CUARTA — CONTROL DE CAMBIOS

4.1. Todo requerimiento **nuevo**, o todo requerimiento que al detallarse **suba
de nivel de complejidad**, se tramita por **control de cambios**: EL CONTRATISTA
lo estima (complejidad y precio) y, si LAS PARTES lo aceptan por escrito, se
formaliza mediante **adenda**. Sin adenda firmada, el cambio no se ejecuta y no se
cobra.

4.2. Un **desacuerdo sobre el nivel de complejidad** de un requerimiento es un
asunto de control de cambios, no de facturación: se resuelve por escrito antes de
ejecutar, nunca al entregar.

## QUINTA — VALOR Y FORMA DE PAGO

5.1. El valor del contrato es la **suma de los requerimientos por su complejidad**,
según el Anexo 2, más los servicios derivados que se contraten (implementación,
migración, capacitación). Los valores son **antes de IVA**; el IVA (19%) y demás
tributos aplicables se facturan por separado.

5.2. El pago se realiza por **hitos** ligados a la aceptación de cada fase
(Cláusula TERCERA). Para cada pago, EL CONTRATISTA radica factura o cuenta de
cobro **adjuntando la constancia de pago de aportes a la seguridad social
integral**.

## SEXTA — PLAZO

El plazo de ejecución es el del **Anexo 2**. Podrá suspenderse de mutuo acuerdo, o
unilateralmente por fuerza mayor o caso fortuito comprobado, prorrogándose por un
tiempo igual al de la suspensión.

## SÉPTIMA — OBLIGACIONES DE LAS PARTES

7.1. **De EL CONTRATISTA:** ejecutar los requerimientos con diligencia y calidad
profesional; mantener informado a EL CONTRATANTE del avance; entregar el código
fuente de la parte a la medida en los términos de la Cláusula OCTAVA; acreditar el
pago de su seguridad social; guardar confidencialidad (Cláusula NOVENA).

7.2. **De EL CONTRATANTE:** designar un **interlocutor único** con capacidad de
decisión; entregar oportunamente la información, los accesos y las definiciones
necesarias; probar y recibir los entregables en los plazos pactados; pagar en las
condiciones acordadas. La **mora del CONTRATANTE en decisiones, accesos o
insumos** suspende los plazos correlativos de EL CONTRATISTA.

## OCTAVA — PROPIEDAD INTELECTUAL *(la cláusula que decide todo)*

La titularidad se define en **tres cubetas separadas**. Confundirlas es el error
que puede costar el negocio.

**8.1. Activos Preexistentes (background) — 100% de EL CONTRATISTA, siempre.**
Los marcos, librerías, motores, *Quipu* y todo elemento de titularidad de EL
CONTRATISTA **anterior o ajeno** a este contrato **NO se ceden ni se licencian**
por la sola suscripción de este contrato. Si un entregable los incorpora, EL
CONTRATANTE recibe únicamente una **licencia de uso** de esos Activos
Preexistentes *en la medida necesaria para operar el entregable*, no su
titularidad. **Esta cubeta nunca se negocia.**

**8.2. Desarrollo a la medida (foreground) — según la modalidad pactada.** Sobre
la parte **desarrollada específicamente** para EL CONTRATANTE, LAS PARTES eligen
UNA en el Anexo 2:

- **[ ] Modalidad A1 — Titularidad de EL CONTRATISTA + licencia al cliente.**
  EL CONTRATISTA conserva la titularidad de los derechos patrimoniales del
  desarrollo a la medida y concede a EL CONTRATANTE una **licencia de uso**
  [perpetua / por [N] años], [no] exclusiva, para sus actividades. Es la
  modalidad **preferente** y de **menor precio** (Anexo 2), porque el resultado
  alimenta el catálogo de EL CONTRATISTA.

- **[ ] Modalidad A2 — Cesión exclusiva a EL CONTRATANTE.** EL CONTRATISTA
  **cede** a EL CONTRATANTE los derechos patrimoniales del desarrollo a la
  medida, con carácter exclusivo. Esta modalidad conlleva el **sobreprecio** del
  Anexo 2, por el lucro cesante de las reventas a las que EL CONTRATISTA renuncia.
  **La cesión se limita al foreground: NUNCA alcanza los Activos Preexistentes
  (8.1).**

- **[ ] Modalidad A3 — Cotitularidad proporcional a los aportes.** La titularidad
  del desarrollo a la medida corresponde a LAS PARTES en proporción a sus aportes.
  **El reparto y el derecho de cada parte a explotar el resultado se definen EN
  ESTE CONTRATO (Anexo 3), no se aplazan.** Cada parte puede explotar el resultado
  según lo pactado en el Anexo 3; la publicación o divulgación requiere aprobación
  previa de ambas.

  > **ADVERTENCIA (#63).** La minuta de SURA establece cotitularidad **con el
  > reparto APLAZADO** a «un acuerdo posterior por escrito». Eso es **peor que una
  > cesión**: deja a Xiliux de codueño de su propio desarrollo con un tercero, y el
  > reparto que nunca se firma congela la explotación. **Si un CLIENTE propone una
  > cláusula así, se rechaza o se exige definir el reparto AQUÍ.** A3 solo se acepta
  > con el Anexo 3 diligenciado en la firma.

**8.3. Datos del CONTRATANTE — siempre de EL CONTRATANTE.** Los datos que EL
CONTRATANTE cargue u opere en la solución son de su exclusiva titularidad. EL
CONTRATISTA actúa como **Encargado del Tratamiento** (Cláusula DÉCIMA) y no
adquiere derecho alguno sobre ellos.

**8.4. Derechos morales.** Son inalienables y permanecen en cabeza del autor
persona natural (art. 30 Ley 23/1982), con independencia de la cubeta 8.2.

> **NOTA AL REDACTOR (crítica).** Sin pacto expreso en 8.2, el art. 20 de la Ley
> 23/1982 (mod. art. 28 Ley 1450/2011) **presume la transferencia del foreground
> al encargante** «en la medida necesaria para sus actividades habituales»,
> siempre que el contrato conste por escrito. Es decir: el silencio equivale a
> A2 sin cobrarla. **Marcar SIEMPRE una modalidad en el Anexo 2**, y por defecto
> A1. La cubeta 8.1 (background) es la que de verdad protege el negocio.

## NOVENA — CONFIDENCIALIDAD

LAS PARTES guardan reserva de la información que conozcan con ocasión del
contrato, no la usan con fines distintos a su ejecución, y mantienen esta
obligación durante su vigencia y **cinco (5) años más**. No aplica a la
información de dominio público, previamente conocida legítimamente, o cuya
revelación exija una autoridad competente (con aviso previo a la otra parte).

## DÉCIMA — PROTECCIÓN DE DATOS PERSONALES

En cuanto EL CONTRATISTA trate datos personales de titulares del CONTRATANTE,
actúa como **Encargado del Tratamiento** en los términos de las Leyes 1266 de
2008 y 1581 de 2012 y el Decreto 1074 de 2015: solo por instrucción del
CONTRATANTE (Responsable), con medidas de seguridad adecuadas, y devolviendo o
suprimiendo los datos al terminar el contrato. Se suscribe el **Anexo de
Tratamiento de Datos** cuando el desarrollo lo requiera.

## DÉCIMA PRIMERA — GARANTÍA Y SOPORTE

11.1. EL CONTRATISTA garantiza que cada entregable cumple los requerimientos
aceptados durante **[noventa (90)] días** desde su aceptación; dentro de ese plazo
corrige sin costo los defectos que sean incumplimiento de lo pactado (no
requerimientos nuevos).

11.2. El **soporte y mantenimiento** posterior es un servicio **aparte** (Anexo 3
y `LICENCIAMIENTO.md §10.8`), con niveles de respuesta por **severidad**
—Crítico, Grave, Moderado, Leve— según la misma escala del Anexo 1. Sin contrato
de soporte vigente, no hay obligación de atención más allá de la garantía.

## DÉCIMA SEGUNDA — LIMITACIÓN DE RESPONSABILIDAD

Salvo dolo o culpa grave, la responsabilidad total de EL CONTRATISTA por cualquier
concepto se limita al **valor efectivamente pagado** en los últimos [doce (12)]
meses del contrato. No responde por lucro cesante, daños indirectos, ni por el uso
que EL CONTRATANTE dé a la solución fuera de lo pactado.

## DÉCIMA TERCERA — INDEPENDENCIA (no relación laboral)

Este es un contrato de **prestación de servicios**. EL CONTRATISTA obra con plena
autonomía técnica y administrativa, sin subordinación ni horario, y asume sus
propios aportes a la seguridad social. **Nada en este contrato configura relación
laboral** entre LAS PARTES.

## DÉCIMA CUARTA — CESIÓN

Ninguna Parte puede ceder el contrato ni los derechos y obligaciones que de él
derivan sin autorización previa, expresa y escrita de la otra.

## DÉCIMA QUINTA — TERMINACIÓN

El contrato termina por: cumplimiento del objeto; mutuo acuerdo; incumplimiento
grave no subsanado dentro de [quince (15)] días de requerido; o imposibilidad por
causa extraña. A la terminación, EL CONTRATISTA entrega lo desarrollado y aceptado
hasta la fecha, y EL CONTRATANTE paga lo efectivamente ejecutado.

## DÉCIMA SEXTA — FUERZA MAYOR

Ninguna Parte responde por el incumplimiento debido a fuerza mayor o caso fortuito
debidamente comprobado.

## DÉCIMA SÉPTIMA — LEY APLICABLE Y CONTROVERSIAS

Se rige por la ley colombiana. Las controversias se resuelven primero por
**arreglo directo** ([treinta (30)] días); de no lograrse, [ante los jueces de la
República / mediante arbitraje en el Centro de [___]].

## DÉCIMA OCTAVA — NOTIFICACIONES

Las comunicaciones se surten a las direcciones y correos indicados en los
COMPARECIENTES.

---

# ANEXO 1 — REQUERIMIENTOS Y CLASIFICACIÓN DE COMPLEJIDAD

| # | Requerimiento | Complejidad | Criterio de aceptación |
|---|---|---|---|
| R1 | [___] | [Leve/Moderado/Grave/Crítico] | [___] |
| R2 | [___] | [___] | [___] |
| … | | | |

Escala (de `LICENCIAMIENTO.md §10.7`): **Leve** 4–12 h · **Moderado** 16–40 h ·
**Grave** 48–100 h · **Crítico** 120–240 h.

# ANEXO 2 — CONDICIONES ECONÓMICAS Y CRONOGRAMA

- **Modalidad de PI (Cláusula 8.2):** [ ] A1  [ ] A2 (+[50–80]%)  [ ] A3
- **Tarifa base:** $[140.000]/h (desarrollo) · $[200.000]/h (consultoría).
- **Precio por requerimiento** = horas de su complejidad × tarifa. **Total:** $[___] + IVA.
- **Hitos de pago:** [Fase 1 al aceptar R1–Rk: $___] · […]
- **Plazo:** inicio [___]; entregas [___].

# ANEXO 3 — SOPORTE Y NIVELES DE SERVICIO *(si se contrata)*

Escala de severidad y tiempos de respuesta: ver `LICENCIAMIENTO.md §10.8`.
Cotización del mantenimiento: [~20% del valor del desarrollo / año, o según plan].

---

# NOTAS AL REDACTOR

1. **La Cláusula OCTAVA es el corazón.** El error caro es ceder el *background*
   junto con el *foreground*. Separar las tres cubetas (8.1/8.2/8.3) es lo que
   mantiene vivo el volante A→B: aunque el cliente exija A2, solo se lleva el
   foreground; los Activos Preexistentes siguen siendo de Xiliux y vuelven al
   catálogo (Vía B). Fundado en el patrón real de la minuta de SURA (cotitularidad
   proporcional a los aportes + background protegido) y en la memoria de cesión de
   patrimoniales (Ley 1450/2011).
2. **Contratación pública (Vía A vía contratación directa, #42):** si el
   CONTRATANTE es una entidad estatal, revisar el régimen de inhabilidades e
   incompatibilidades (Ley 80/1993, Ley 1150/2007) y NO ofertar en la Vía B de la
   misma entidad si se participó en sus estudios previos.
3. **Modalidad por defecto: A1.** Cotizar siempre A1 y A2 para que el cliente vea
   el costo de exigir exclusividad.
4. Este documento es un **modelo de referencia**, no un contrato impuesto; se
   instancia y ajusta por cliente (coherente con la decisión 0001 sobre el EULA).
