# Anclaje de precios contra contratación pública real

**2026-07-21.** Reproducible con `python analisis_precios_secop.py`.
**Comprobado**: dos corridas desde cero dan cifras idénticas. La primera versión
del script NO era reproducible —la paginación de Socrata sin `$order` explícito
repite y pierde filas, y dos corridas daban medianas de $37,1M y $35,6M—.
Corregido con orden explícito; el 4% de deriva no se notaba, que es lo peor. Ningún
número de este documento está escrito a mano: todos salen de ese script contra
la API de datos abiertos de SECOP II (dataset `jbjy-vk9h`).

---

## Primero, la corrección: el análisis anterior estaba sesgado

El análisis del 2026-07-18 pidió a la API **los 200 contratos de mayor valor** y
calculó la mediana sobre ellos. Eso no es la mediana del mercado: es la del
tramo alto. De ahí salió la conclusión de que los precios propuestos estaban
«bajos por un orden de magnitud».

**Esa conclusión era falsa, y de las caras.** Subir precios sobre una medición
sesgada habría dejado el producto fuera de mercado sin que nadie supiera por qué.

Cuando una muestra se ordena por la variable que se va a medir, deja de ser una
muestra. Aquí se usa el universo completo que cumple el filtro.

---

## 1. Software de seguimiento — el comparable directo

**2.624 contratos** (de 2.646 descargados; 22 sin valor legible).

| | |
|---|---|
| p25 | $18,1M |
| **mediana** | **$36,5M** |
| p75 | $66,1M |
| p90 | $118,0M |

Los contratos de $2.000M–$6.464M que citaba el análisis anterior existen, pero
son el percentil 95 y superior. Compararse con ellos es compararse con
integraciones multianuales con equipo detrás.

## 2. Una persona haciendo el trabajo a mano — el competidor real

**3.344 contratos**, anualizados. La anualización no es un detalle: la duración
mediana es de **180 días**, así que comparar valores sin normalizar subestimaría
el coste anual a la mitad.

| | |
|---|---|
| p25 | $42,6M / año |
| **mediana** | **$54,8M / año** |
| p75 | $73,6M / año |
| p90 | $99,2M / año |

> **`LICENCIAMIENTO.md` ya estimaba «~$54.000.000 al año» para un analista de
> cumplimiento. La mediana medida sobre 3.344 contratos públicos reales es
> $54,8M.** La estimación era correcta; ahora es verificable.

## 3. Por dónde se contrata — y por qué importa

| Modalidad | Cuota |
|---|---|
| **Contratación directa** | **85,4%** |
| Contratación régimen especial | 5,3% |
| Mínima cuantía | 3,2% |
| Contratación directa (con ofertas) | 2,5% |
| Licitación pública | 0,4% |

**El 88% del mercado entra por contratación directa o mínima cuantía**, y el
artículo 6 de la Ley 1150 de 2007 **exime a ambas del RUP**.

Eso responde el temor de la tarea —que «experiencia acreditada en contratos
similares» dejara fuera al primer cliente—: en la puerta por la que entra el 85%
de este mercado, ese requisito no es el filtro. Solo 10 contratos de 2.646
fueron por licitación pública.

---

## 4. El anclaje

| Edición | Precio | Percentil del software | Equivale a |
|---|---|---|---|
| Esencial | $12M | 12 | **2,6 meses** de una persona |
| Profesional | $28M | 40 | **6,1 meses** de una persona |
| Corporativa | $55M | 68 | **12,1 meses** de una persona |

**La Corporativa perpetua cuesta lo mismo que UN AÑO de UNA persona.** Ese es el
argumento, y ahora es una cifra medida, no una impresión.

### Conclusión, que corrige a la tarea

**Los precios NO están bajos por un orden de magnitud.** Se sitúan entre los
percentiles 12 y 68 del mercado real de software comparable — una posición
razonable para quien entra sin trayectoria acreditada.

Lo que sí conviene revisar es la **Esencial a $12M**: en el percentil 12 compite
con contratos de mínima cuantía donde el margen es estrecho y el cliente que
más soporte pide por dólar. Es la misma razón por la que en Quipu OPRF se
retiró el tramo de 9 USD.

---

## Límites de este análisis, que hay que decir

- **El filtro es por texto libre.** `descripcion_del_proceso` es como la entidad
  redactó el objeto; hay contratos relevantes que no dicen «software» ni
  «seguimiento», y otros que los dicen sin serlo. El universo es una
  aproximación, no un censo.
- **El valor del contrato no es el precio de la licencia.** Casi siempre incluye
  implementación, migración, capacitación y soporte. Es el ancla de negociación
  —lo que la entidad YA pagó por el mismo problema— no un comparable de producto.
- **No mide qué se entregó.** Un contrato de $2.000M puede haber sido un fracaso.
- **Los contratos de persona incluyen prestaciones** en la contratación estatal
  de servicios profesionales; el coste real para la entidad puede ser mayor.

## Lo que este análisis NO puede responder

Los **pliegos y estudios previos** no están en este dataset: para ver requisitos
técnicos, experiencia mínima y garantías exigidas hay que bajarlos de SECOP II
por proceso. La modalidad de contratación es un buen indicador —y es el que se
usa aquí— pero no sustituye a leer un pliego.
