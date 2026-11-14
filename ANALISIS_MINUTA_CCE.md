# Análisis del documento tipo de CCE — compraventa de software

**Fuente:** documento tipo de Colombia Compra Eficiente, "Contrato para la
compraventa de Software" (33 cláusulas). Minuta oficial en blanco.
**Posición analizada:** la de Xiliux como **Contratista** (vendedor).
**Fecha:** 2026-07-18.

> El propio documento dice que "debe adaptarse a las condiciones particulares del
> Proceso de Contratación". No es intocable: es un punto de partida. Esa frase es
> la palanca de toda negociación que sigue.

---

## 1. El hallazgo principal: no hay cláusula de propiedad intelectual

**Verificado por búsqueda en el texto completo.** No existe cláusula de propiedad
intelectual, ni de derechos patrimoniales, ni de derecho de autor, ni de código
fuente. Las únicas apariciones de la raíz "licenc" son la definición de *Licencia*
y la cláusula 5 de mantenimiento.

Esto es peligroso por acumulación de tres cosas:

1. El contrato se titula **compraventa**, figura que en su sentido natural
   transfiere la propiedad de la cosa vendida.
2. No hay salvaguarda de propiedad intelectual **preexistente** — la que sí trae
   el contrato de la EAN.
3. La presunción del art. 28 de la Ley 1450 de 2011 corre a favor del encargante
   cuando hay obra creada en ejecución del contrato.

Aisladamente cada punto es discutible; juntos, dejan abierta la puerta a que una
entidad sostenga que adquirió algo más que un derecho de uso.

**Qué exigir:** una cláusula nueva, calcada de la salvaguarda de la EAN, que diga
que el Software es propiedad intelectual preexistente del Contratista y que sobre
ella no se entiende cesión ni transferencia alguna. Es la cláusula 8.5 del EULA.
No se negocia.

## 2. La Licencia del documento tipo es a plazo, no perpetua

La definición dice: *"Licencia: es el derecho de uso del software por un plazo de
[insertar el periodo]"*.

El modelo de `informes` es **perpetuo + mantenimiento anual**. La minuta asume
licencia temporal. Hay que reescribir la definición, o el contrato dirá que la
licencia caduca — justo lo contrario de lo que se vende.

## 3. Asimetría de derechos (esperable, pero hay que verla escrita)

| | |
|---|---|
| **Derechos del Contratista** (cl. 9) | Uno: cobrar. |
| **Derechos del Contratante** (cl. 11) | Cláusulas excepcionales, multas, cláusula penal, "cualquier otro derecho". |

Sumado a las potestades exorbitantes de la cl. 13 —terminar, modificar **e
interpretar** unilateralmente el contrato, arts. 15 a 17 de la Ley 80 de 1993— y
a la liquidación unilateral de la cl. 32.

Esto es propio de la contratación estatal y **no se puede eliminar**: son
potestades legales. Pero conviene entrar sabiendo que existen, y que el arbitraje
**no las cubre** (la cl. 23 las excluye expresamente del tribunal de
arbitramento: van a la jurisdicción contencioso administrativa).

## 4. Dos riesgos económicos concretos

### 4.1 La cláusula penal es un piso, no un techo

Cl. 15: *"la presente cláusula no impide el cobro de todos los perjuicios
adicionales que se causen sobre el citado valor"*. Y ese valor **puede
compensarse** contra lo que la entidad te adeude.

Es decir: no hay tope de responsabilidad, y el cobro puede ejecutarse
descontándolo de tus facturas pendientes.

**Qué pedir:** que la cláusula penal sea estimación **única y definitiva** de
perjuicios, con tope. Es negociable y es estándar pedirlo.

### 4.2 La exclusión de daños indirectos está redactada de forma que casi no
protege

Cl. 12: *"Ninguna de las partes será responsable […] por daños especiales,
imprevisibles o daños indirectos, **derivados de fuerza mayor o caso fortuito**"*.

Ese calificativo final vacía la cláusula: solo excluye los daños indirectos que
provengan de fuerza mayor, que ya están excluidos por la cl. 22. En un
incumplimiento ordinario, **no hay límite de daños indirectos**.

**Qué pedir:** suprimir "derivados de fuerza mayor o caso fortuito", de modo que
la exclusión de daños indirectos opere con carácter general, y añadir un tope
cuantitativo referido al valor del contrato.

## 5. La garantía de cumplimiento (cl. 17) es la barrera real de entrada

El contrato **no se perfecciona** sin ella (cl. 28): firma + paz y salvo de
seguridad social + **aprobación de la garantía**.

Es una póliza de una aseguradora, y es exactamente el obstáculo de capacidad
financiera que ya habíamos identificado para SECOP II. **Ninguna cláusula
inteligente lo evita**: hay que resolverlo antes, no negociarlo después.

→ Pregunta para la llamada a CCE del martes.

## 6. Lo que la minuta no trae y `informes` sí necesita

- **Niveles de servicio (ANS)** — no hay ninguno. Sin ANS definido, la
  "inoperatividad del software" de la cl. 16 la califica la entidad, y es causal
  de **caducidad**.
- **Protección de datos personales** — no hay cláusula de Ley 1581 de 2012, pese
  a que el software tratará datos. La cl. 31 solo cubre "información sujeta a
  reserva legal" y exige avisar que es confidencial.
- **Escrow / depósito de código** — no previsto. Conviene ofrecerlo: responde a
  "¿qué pasa si Xiliux desaparece?" sin entregar el código.
- **Aceptación y pruebas de entrega** — no hay procedimiento. Sin él, el momento
  en que la obligación se entiende cumplida queda indefinido.

## 7. Erratas del documento tipo (heredadas si se firma tal cual)

- La cláusula **16** numera sus literales como `15.1` y `15.2`.
- La cláusula **27** numera sus anexos como `26.1`, `26.2`, `26.3`.

Menores, pero se corrigen al adaptar: un contrato con la numeración cruzada
invita a discusiones sobre a qué remite cada cosa.

---

## Resumen accionable

| Prioridad | Punto | Acción |
|---|---|---|
| **Innegociable** | Sin cláusula de PI (§1) | Añadir salvaguarda de PI preexistente |
| **Alta** | Licencia a plazo (§2) | Redefinir como perpetua + mantenimiento |
| **Alta** | Penal sin tope (§4.1) | Estimación única y definitiva, con tope |
| **Alta** | Daños indirectos (§4.2) | Suprimir el calificativo; añadir tope |
| **Bloqueante** | Garantía (§5) | Resolver antes de licitar — llamada CCE |
| **Media** | Sin ANS (§6) | Anexar ANS; define qué es "inoperatividad" |
| **Media** | Sin Ley 1581 (§6) | Añadir cláusula de tratamiento de datos |

**Lectura de conjunto:** la minuta no es abusiva, es *estatal* — escrita desde la
posición del comprador público y con las potestades que la ley le da. Lo que la
hace riesgosa para Xiliux no es lo que dice, sino **lo que calla**: la propiedad
intelectual. Ese silencio, en un contrato titulado "compraventa", es el que hay
que cerrar antes de cualquier otra cosa.
