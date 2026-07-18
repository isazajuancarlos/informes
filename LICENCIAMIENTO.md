# Modelo de licenciamiento y precios — Informes

**Documento interno de decisión comercial. No se entrega al cliente.**
Lo que sí se entrega es el EULA y la cotización, que se derivan de aquí.

Versión 1.0 · 2026-07-18 · Propuesta para revisión de Juan Carlos Isaza

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

## 4. Precios

> **Cifras en COP.** El equivalente en USD es referencial a ~4.000 COP/USD;
> confírmalo antes de cotizar en moneda extranjera. Todos los valores son
> **antes de IVA**.

### 4.1. Licencia perpetua — `standalone` y `onprem`

| Edición | Perfil | Alcance | Licencia perpetua | Mantenimiento anual (20%) |
|---|---|---|---|---|
| **Esencial** | `standalone` | Hasta 25 usuarios · 1 sede · SQLite | **$12.000.000** | $2.400.000 |
| **Profesional** | `onprem` | Hasta 100 usuarios · multi-área · PostgreSQL | **$28.000.000** | $5.600.000 |
| **Corporativa** | `onprem` | Usuarios ilimitados · multi-sede · air-gapped | **$55.000.000** | $11.000.000 |

**Año 1 = licencia + mantenimiento incluido.** Es decir, la Profesional se
factura en **$33.600.000** el primer año y **$5.600.000** cada año siguiente.

### 4.2. Suscripción — `cloud`

| Edición | Alcance | Mensual | Anual (2 meses de descuento) |
|---|---|---|---|
| **Cloud Esencial** | Hasta 25 usuarios | $850.000 | $8.500.000 |
| **Cloud Profesional** | Hasta 100 usuarios | $1.950.000 | $19.500.000 |

Incluye hospedaje, respaldos, actualizaciones y disponibilidad. Sin licencia
inicial. El precio anual está calibrado para que **la suscripción se cruce con
la compra perpetua alrededor del año 3**: antes de eso el cloud es más barato,
después el perpetuo lo es. Esa es la conversación honesta con el cliente y la
que además empuja a las empresas grandes hacia el perpetuo, que es donde el
producto encaja mejor.

### 4.3. Servicios, aparte de la licencia

| Concepto | Precio | Nota |
|---|---|---|
| Implementación y parametrización | $6.000.000 – $18.000.000 | Según nº de reglas y variables |
| Migración de datos históricos | Desde $4.500.000 | Cotizar por volumen y estado del origen |
| Capacitación (sesión de 4 h, hasta 15 personas) | $1.800.000 | Presencial en Medellín sin recargo |
| Desarrollo a la medida | $320.000 / hora | Mínimo 20 horas |
| Instalación air-gapped asistida en sitio | $3.500.000 | Viáticos aparte fuera del Valle de Aburrá |

**La implementación no se regala.** Es donde se va el tiempo real y es lo que un
cliente entiende que se paga.

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
