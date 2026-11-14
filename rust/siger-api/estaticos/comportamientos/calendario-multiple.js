// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
//
// calendario-multiple — marcar VARIAS fechas sueltas en una rejilla de mes.
//
//     <textarea data-calendario-multiple name="fechas">2026-01-15
//     2026-02-15</textarea>
//
// ── POR QUÉ EXISTE ───────────────────────────────────────────────────────────
//
// Lo tenía Angular y se perdió al migrar a `maud` + htmx. En `informes`, la
// pantalla de cronograma pregunta «¿qué regla explica estas fechas?»: se marcan
// unos días y el motor propone las recurrencias que los explican. Al retirar
// Angular la FUNCIÓN se conservó entera y el gesto no: hoy las fechas se
// escriben a mano, `2026-01-15` una por línea. Da la misma respuesta y se usa
// peor — y es la pantalla que se enseña en la demostración de venta, minuto y
// medio de los cinco.
//
// El original de Angular estaba escrito a mano por la misma razón por la que
// esto también lo está: el selector de fechas de Material no admite selección
// múltiple, y la librería que sí la traía —PrimeNG— dejó de ser MIT. Aquí el
// motivo se mantiene y se refuerza: una dependencia de npm para esto es
// exactamente el árbol que este stack existe para no tener.
//
// ── EL CAMPO SIGUE SIENDO LA FUENTE DE VERDAD ────────────────────────────────
//
// El calendario NO sustituye al campo: se dibuja al lado y lo edita. Es el
// principio de boi —el HTML es la única fuente de verdad— y tiene una
// consecuencia que vale más que la comodidad: **sin JavaScript, la pantalla
// sigue funcionando exactamente como hoy**, escribiendo. No hay un modo
// degradado que construir, porque el modo degradado es el estado actual.
//
// Por eso la sincronización va en los dos sentidos: marcar un día reescribe el
// campo, y escribir en el campo repinta la rejilla.
//
// ── ACCESIBILIDAD, DESDE EL PRIMER COMMIT ────────────────────────────────────
//
// Cada día es un `<button type="button">` con su fecha completa como nombre
// accesible y `aria-pressed` diciendo si está marcado. Una rejilla de `<div>`
// con `onclick` habría quedado igual en pantalla y muda para un lector — y en
// `informes` hay un banco que mide justo eso contra el árbol de accesibilidad
// del navegador, así que aquí no es una intención, es una condición.
//
// El `type="button"` NO es cosmético: sin él, un `<button>` dentro de un
// formulario lo ENVÍA al pulsarlo, y marcar el segundo día mandaría el
// formulario a medio llenar.

import { registrar } from "../boi.js";

const DIAS = ["lu", "ma", "mi", "ju", "vi", "sá", "do"];
const MESES = [
  "enero", "febrero", "marzo", "abril", "mayo", "junio",
  "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre",
];

/// Las fechas que hay en el texto del campo, en ISO y sin repetir.
///
/// Se aceptan los mismos separadores que el servidor acepta —salto de línea,
/// coma y punto y coma—, porque el campo es la fuente de verdad y quien escriba
/// a mano no tiene por qué saber cuál eligió el calendario. Lo que no parece una
/// fecha ISO **se ignora en silencio AQUÍ y solo aquí**: esto pinta una rejilla,
/// y el rechazo ruidoso de lo ilegible es del dominio, en el servidor, que es
/// quien puede decirlo bien (directiva 20 — el permisivo lo pide el llamante, y
/// este llamante solo dibuja).
export function fechasDe(texto) {
  const vistas = new Set();
  for (const trozo of String(texto ?? "").split(/[\n,;]/)) {
    const f = trozo.trim();
    if (/^\d{4}-\d{2}-\d{2}$/.test(f) && esFechaReal(f)) vistas.add(f);
  }
  return vistas;
}

/// `2026-02-30` casa la forma y no existe. Se comprueba yendo y volviendo: si
/// `Date` normalizó el día, la fecha era imposible.
function esFechaReal(iso) {
  const [a, m, d] = iso.split("-").map(Number);
  const t = new Date(Date.UTC(a, m - 1, d));
  return t.getUTCFullYear() === a && t.getUTCMonth() === m - 1 && t.getUTCDate() === d;
}

/// El texto que se escribe de vuelta en el campo: una fecha por línea y en
/// orden. Ordenadas porque el campo se lee con los ojos y un montón de fechas
/// sin orden no se revisa; en ISO el orden alfabético ES el cronológico.
export function textoDe(fechas) {
  return [...fechas].sort().join("\n");
}

/// La rejilla de un mes: seis semanas de siete celdas, cada una con su ISO o
/// `null` si cae fuera del mes.
///
/// **La semana empieza en LUNES**, que es la convención de Colombia y la del
/// calendario que se retiró. Y el número de semanas es fijo (6): con un número
/// variable la rejilla cambia de alto al pasar de mes y los días bailan bajo el
/// cursor, que es la clase de detalle que hace que un control no se use.
export function rejilla(anio, mes) {
  const primero = new Date(Date.UTC(anio, mes, 1));
  // `getUTCDay()` da 0 para domingo; con la semana en lunes, domingo es el 6.
  const hueco = (primero.getUTCDay() + 6) % 7;
  const semanas = [];
  let dia = 1 - hueco;
  for (let s = 0; s < 6; s++) {
    const semana = [];
    for (let c = 0; c < 7; c++, dia++) {
      const t = new Date(Date.UTC(anio, mes, dia));
      semana.push(t.getUTCMonth() === mes ? iso(t) : null);
    }
    semanas.push(semana);
  }
  return semanas;
}

/// ISO en UTC y a mano. `toISOString().slice(0,10)` valdría, pero construir con
/// `new Date(a, m, d)` —hora LOCAL— y formatear en UTC corre el día para quien
/// esté al oeste de Greenwich, que es todo el país. Aquí todo es UTC de punta a
/// punta y no hay conversión que equivocarse.
function iso(t) {
  const p = (n) => String(n).padStart(2, "0");
  return `${t.getUTCFullYear()}-${p(t.getUTCMonth() + 1)}-${p(t.getUTCDate())}`;
}

/// El mes que conviene enseñar al abrir: el de la primera fecha ya marcada, y si
/// no hay ninguna, el actual. Abrir siempre en el mes de hoy obligaría a
/// navegar hacia atrás para ver lo que uno mismo acaba de escribir.
export function mesInicial(fechas, hoy) {
  const primera = [...fechas].sort()[0];
  if (primera) {
    const [a, m] = primera.split("-").map(Number);
    return { anio: a, mes: m - 1 };
  }
  return { anio: hoy.getUTCFullYear(), mes: hoy.getUTCMonth() };
}

registrar("calendario-multiple", "[data-calendario-multiple]", (campo) => {
  const doc = campo.ownerDocument;
  const caja = doc.createElement("div");
  caja.className = "boi-calendario";
  caja.setAttribute("role", "group");
  campo.after(caja);

  let { anio, mes } = mesInicial(fechasDe(campo.value), new Date());

  const pintar = () => {
    const marcadas = fechasDe(campo.value);
    caja.setAttribute("aria-label", `Calendario: ${MESES[mes]} de ${anio}`);
    caja.textContent = "";

    const cabecera = doc.createElement("div");
    cabecera.className = "boi-calendario-mes";
    cabecera.append(
      boton(doc, "‹", `Mes anterior`, () => mover(-1)),
      rotulo(doc, `${MESES[mes]} ${anio}`),
      boton(doc, "›", `Mes siguiente`, () => mover(1)),
    );
    caja.append(cabecera);

    const tabla = doc.createElement("table");
    const thead = doc.createElement("thead");
    const fila = doc.createElement("tr");
    for (const d of DIAS) {
      const th = doc.createElement("th");
      th.scope = "col";
      th.textContent = d;
      fila.append(th);
    }
    thead.append(fila);
    tabla.append(thead);

    const tbody = doc.createElement("tbody");
    for (const semana of rejilla(anio, mes)) {
      const tr = doc.createElement("tr");
      for (const dia of semana) {
        const td = doc.createElement("td");
        if (dia) {
          const b = boton(
            doc,
            String(Number(dia.slice(8))),
            // El nombre accesible es la fecha ENTERA, no el número: un lector de
            // pantalla que anuncia «15» dentro de una tabla no dice de qué mes.
            `${Number(dia.slice(8))} de ${MESES[mes]} de ${anio}`,
            () => alternar(dia),
          );
          b.setAttribute("aria-pressed", marcadas.has(dia) ? "true" : "false");
          if (marcadas.has(dia)) b.classList.add("marcado");
          td.append(b);
        }
        tr.append(td);
      }
      tbody.append(tr);
    }
    tabla.append(tbody);
    caja.append(tabla);
  };

  const mover = (n) => {
    const t = new Date(Date.UTC(anio, mes + n, 1));
    anio = t.getUTCFullYear();
    mes = t.getUTCMonth();
    pintar();
  };

  const alternar = (dia) => {
    const marcadas = fechasDe(campo.value);
    if (marcadas.has(dia)) marcadas.delete(dia);
    else marcadas.add(dia);
    campo.value = textoDe(marcadas);
    // `input` y no `change`: es el que escuchan tanto htmx como cualquier
    // validación en vivo, y el que dispara el navegador al teclear. Sin él, el
    // campo cambia y nadie se entera.
    campo.dispatchEvent(new Event("input", { bubbles: true }));
    pintar();
  };

  // El otro sentido: quien teclee en el campo ve la rejilla seguirle. Sin esto
  // habría dos verdades en pantalla, y la de abajo sería la vieja.
  campo.addEventListener("input", pintar);

  pintar();
});

function boton(doc, texto, nombre, alPulsar) {
  const b = doc.createElement("button");
  // Ver la cabecera: sin `type="button"` cada día enviaría el formulario.
  b.type = "button";
  b.textContent = texto;
  b.setAttribute("aria-label", nombre);
  b.addEventListener("click", (e) => {
    e.preventDefault();
    alPulsar();
  });
  return b;
}

function rotulo(doc, texto) {
  const s = doc.createElement("span");
  s.className = "boi-calendario-rotulo";
  s.textContent = texto;
  return s;
}
