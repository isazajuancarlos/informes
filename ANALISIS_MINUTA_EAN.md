# Análisis del contrato de la Universidad EAN — licenciamiento y servicios

**Fuente:** "Contrato para licenciamiento de software y servicios adicionales",
Universidad EAN (NIT 860.026.058-1), 28 cláusulas. Minuta del lado del comprador,
con la razón social del contratista en blanco.
**Posición analizada:** la de Xiliux como **CONTRATISTA**.
**Fecha:** 2026-07-18.

> Es el mismo archivo que `EAN_contrato_licenciamiento_y_servicios.pdf` en la
> carpeta de minutas (MD5 `bc95dd70…`, texto idéntico).

**Por qué importa más que la minuta de CCE:** la EAN es una universidad
**privada**. Esto es lo que un cliente institucional privado pone a firmar — la
vía que estamos priorizando (#41). No tiene potestades exorbitantes, pero tampoco
las contrapartidas de la Ley 80.

---

## 1. La combinación más peligrosa: licencia atada al plazo + salida libre

Dos cláusulas que por separado son normales y juntas destruyen el modelo:

- **Cl. 1.1:** el objeto es *"otorgar el derecho de uso **durante la vigencia del
  presente contrato**"*. Licencia temporal, no perpetua.
- **Cl. 14.4:** LA EAN puede terminar *"dando aviso […] con treinta (30) días
  calendario de antelación […] **sin que haya lugar a indemnización alguna**"*.

Resultado: el cliente puede cancelar a los 30 días, sin causa y sin indemnizar, y
la licencia muere con el contrato. Si la implantación costó tres meses de trabajo,
se pierde.

**Qué exigir**, en este orden:

1. Que la licencia sea **perpetua** e independiente de la vigencia del contrato de
   servicios. Lo que termina es el mantenimiento, no el derecho de uso.
2. Si no aceptan (1): que la terminación sin causa de la cl. 14.4 **no sea
   aplicable durante el primer año**, o que devengue el saldo pendiente.

Este punto vale más que todos los demás juntos.

## 2. La cláusula penal es asimétrica por su parágrafo

Cl. 9: pena del **20 % del valor total**, exigible ejecutivamente, sin
requerimiento ni constitución en mora, *"sin perjuicio de exigir las demás
prestaciones"* y *"sin que se extinga la obligación principal"*. Descontable de lo
que te adeuden.

Aparenta ser bilateral. No lo es, por el parágrafo único: **en obligaciones
dinerarias el retardo no activa la pena**, solo intereses moratorios.

La obligación principal de LA EAN **es dineraria** — pagar. Así que en la práctica
la pena del 20 % solo puede caer sobre ti. Es una exención recíproca solo en
apariencia.

> Además hay un error de redacción: dice *"exigirle al **contratante**
> incumplido"*, cuando el encabezado habla de "cualquiera de las partes".
> Conviene corregirlo: la ambigüedad no juega a favor de quien no redactó.

**Qué pedir:** que la pena sea estimación **única y definitiva** de perjuicios
(suprimir el "sin perjuicio de"), y que se aplique a ambas partes por igual.

## 3. No hay ninguna limitación de responsabilidad

**Verificado por búsqueda en el texto completo**: no aparece limitación de
responsabilidad, ni tope, ni exclusión de daños indirectos, ni de lucro cesante.

Sumado a la pena del 20 % que no extingue la obligación principal, y a la
obligación de indemnizar *"todos los perjuicios […] que no fuesen cubiertos por la
póliza o en exceso de esta"* (cl. 10, parágrafo primero), la exposición es
**ilimitada**.

Para una persona natural esto significa patrimonio personal. Es el punto que más
conviene negociar después del §1.

**Qué pedir:** tope de responsabilidad referido al valor del contrato, y exclusión
de daños indirectos y lucro cesante.

## 4. Las garantías: la misma barrera financiera, con nombre propio

Cl. 10: incorporarse al "Programa de Grandes Beneficiarios" de LA EAN a través de
su intermediario **RLpartners Ltda**, entregando SARLAFT y anexos, experiencia, y
**suscribiendo las contragarantías necesarias**, en **3 días calendario** desde la
firma.

Si no puedes incorporarte, debes constituir pólizas equivalentes por tu cuenta con
aseguradora aceptable para LA EAN.

Es el mismo obstáculo que en la vía pública, y confirma que **no es un problema
del sector público sino del tamaño del comprador**. Una contragarantía suele
implicar respaldo patrimonial. Hay que resolverlo antes de sentarse a firmar.

→ Ampliar la pregunta del martes: no solo qué exige el Estado, sino cómo accede un
independiente a pólizas de cumplimiento.

## 5. Confidencialidad: amplia, unilateral y sin plazo

- **Unilateral.** Verificado: solo obliga a EL CONTRATISTA. LA EAN no asume
  obligación recíproca de confidencialidad.
- **Sin plazo.** *"se extienden hasta después de la terminación"*, sin decir
  cuánto.
- **Definición extensísima**: *"procesos, métodos de operación, datos, recursos,
  gráficas, información visual, verbal o escrita, procedimientos, ideas, planos,
  diseños, fórmulas, cálculos, estrategias de mercado, precios, contratos, nombres
  y datos de clientes"*.
- Obliga a **no utilizar** *"cualquier clase de datos, productos, estrategias o
  conocimiento que haga parte de los negocios de LA EAN"*.

Ese último verbo es el problema. Redactado así, un cliente podría sostener que
mejoras hechas a `informes` durante la implantación quedan restringidas.

**Qué pedir:** (i) reciprocidad; (ii) plazo determinado (3 o 5 años); (iii)
excepción expresa de **conocimiento residual** y de la propiedad intelectual
preexistente o desarrollada de forma independiente.

## 6. Lo bueno, que conviene reconocer

- **Cl. 20 (PI):** trae la salvaguarda de propiedad intelectual preexistente. Es
  la cláusula que copiamos al EULA como 8.5. En esto la EAN es **mejor** que el
  documento tipo de CCE, que simplemente calla.
- **Cl. 15:** no exclusividad expresa. Puedes vender a otros.
- **Cl. 11:** controversias por la Ley 640 de 2001, **sin** requisito de
  procedibilidad convencional. Deja la vía judicial abierta sin trámite previo
  forzoso.

## 7. Un detalle que conecta con el EULA

**Cl. 24 (contrato único):** prevalece sobre cualquier otro documento *"salvo que
tenga fecha posterior y sea firmado por ambas partes"*.

Confirma que la corrección de la cláusula 8.4 del EULA iba en la dirección
correcta: no basta con fijar la regla, hay que exigir que el documento posterior
**se refiera expresamente** a ella. Con una cláusula de contrato único como esta,
cualquier otrosí firmado después desplaza lo pactado sin necesidad de mencionarlo.

---

## Resumen accionable

| Prioridad | Punto | Acción |
|---|---|---|
| **Crítico** | Licencia atada al plazo + salida a 30 días sin indemnizar (§1) | Licencia perpetua, o blindaje del primer año |
| **Crítico** | Sin límite de responsabilidad (§3) | Tope al valor del contrato; excluir indirectos y lucro cesante |
| **Alta** | Pena del 20 % asimétrica (§2) | Estimación única y definitiva; simetría real |
| **Bloqueante** | Contragarantías en 3 días (§4) | Resolver el acceso a pólizas ANTES de negociar |
| **Alta** | Confidencialidad unilateral y sin plazo (§5) | Reciprocidad, plazo, y excepción de conocimiento residual |
| — | PI preexistente (§6) | Ya está bien. Conservarla |

**Lectura de conjunto:** comparada con la de CCE, esta minuta **acierta justo
donde la estatal calla** (propiedad intelectual) y **falla donde la estatal
protege** (no hay límite de responsabilidad, y la salida sin indemnización es
libre). No conviene tomar ninguna de las dos como plantilla: lo que hay que armar
es el pliego propio de Xiliux con lo mejor de cada una, y llevarlo a la mesa
primero. Quien lleva el borrador define el terreno.
