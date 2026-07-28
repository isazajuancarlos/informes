/**
 * Un calendario en el que se marcan VARIAS fechas.
 *
 * # Por qué está escrito a mano
 *
 * El selector de fechas de Material no admite selección múltiple, y la librería
 * que sí la trae —PrimeNG— dejó de ser MIT: hoy exige clave de licencia y, sin
 * ella, estampa un cartel de «licencia inválida» encima de la aplicación. En un
 * producto que se instala en casa del cliente y corre años sin conexión, una
 * clave que caduca es un defecto que aparece solo, en la pantalla de otro.
 *
 * Son ciento y pico líneas. Cambiar una dependencia con condiciones por ciento y
 * pico líneas propias es el negocio del año.
 *
 * # Qué resuelve, que no es poco
 *
 * Quien crea una obligación TIENE las fechas —están en la resolución, en el
 * contrato— y lo que no tiene es el nombre de la abstracción. Pedirle que
 * traduzca «los primeros cinco días hábiles» a una configuración es pedirle que
 * haga el trabajo del sistema. Aquí marca lo que tiene delante.
 */
import { Component, computed, input, model, signal } from '@angular/core';
import { MatButtonModule } from '@angular/material/button';
import { MatIconModule } from '@angular/material/icon';

interface Casilla {
  fecha: Date | null;
  iso: string;
}

const DIAS = ['Lun', 'Mar', 'Mié', 'Jue', 'Vie', 'Sáb', 'Dom'];
const MESES = [
  'enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio',
  'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre',
];

/** ISO local. `toISOString()` en Bogotá resta cinco horas y convierte el día 1
 *  en el 31 del mes anterior; en un sistema de fechas límite ese desfase es un
 *  incumplimiento inventado. */
export function isoLocal(f: Date): string {
  const p = (n: number) => String(n).padStart(2, '0');
  return `${f.getFullYear()}-${p(f.getMonth() + 1)}-${p(f.getDate())}`;
}

@Component({
  selector: 'siger-calendario-multiple',
  imports: [MatButtonModule, MatIconModule],
  template: `
    <div class="cabecera">
      <button mat-icon-button type="button" (click)="mover(-1)" aria-label="Mes anterior">
        <mat-icon>chevron_left</mat-icon>
      </button>
      <strong>{{ titulo() }}</strong>
      <button mat-icon-button type="button" (click)="mover(1)" aria-label="Mes siguiente">
        <mat-icon>chevron_right</mat-icon>
      </button>
    </div>

    <div class="rejilla encabezado" aria-hidden="true">
      @for (d of dias; track d) { <span>{{ d }}</span> }
    </div>

    <div class="rejilla" role="grid">
      @for (c of casillas(); track c.iso) {
        @if (c.fecha) {
          <button
            type="button"
            role="gridcell"
            class="dia"
            [class.marcado]="marcadas().includes(c.iso)"
            [class.hoy]="c.iso === hoy"
            [attr.aria-pressed]="marcadas().includes(c.iso)"
            (click)="alternar(c.iso)"
          >
            {{ c.fecha.getDate() }}
          </button>
        } @else {
          <span class="hueco"></span>
        }
      }
    </div>

    @if (marcadas().length > 0) {
      <div class="pie">
        <span>{{ marcadas().length }} fechas marcadas</span>
        <button mat-button type="button" (click)="marcadas.set([])">Limpiar</button>
      </div>
    }
  `,
  styles: `
    :host { display: block; max-width: 22rem; }
    .cabecera { display: flex; align-items: center; justify-content: space-between; }
    .rejilla { display: grid; grid-template-columns: repeat(7, 1fr); gap: .15rem; }
    .encabezado span {
      text-align: center; font-size: .7rem; padding: .3rem 0;
      color: var(--mat-sys-on-surface-variant);
    }
    .dia {
      aspect-ratio: 1; border: 0; border-radius: 50%; cursor: pointer;
      background: transparent; color: inherit; font: inherit; font-size: .85rem;
    }
    .dia:hover { background: var(--mat-sys-surface-variant); }
    /* El día de hoy se señala con un borde, no con relleno: el relleno es lo que
       significa «marcado», y usarlo para dos cosas hace que una obligación
       parezca marcada cuando no lo está. */
    .hoy { outline: 1px solid var(--mat-sys-outline); }
    .marcado {
      background: var(--mat-sys-primary); color: var(--mat-sys-on-primary); font-weight: 600;
    }
    .marcado:hover { background: var(--mat-sys-primary); }
    .pie {
      display: flex; align-items: center; justify-content: space-between;
      margin-top: .5rem; font-size: .85rem; color: var(--mat-sys-on-surface-variant);
    }
  `,
})
export class CalendarioMultiple {
  /** Fechas marcadas, en ISO local. Es un `model` para que la pantalla que lo
   *  usa lea y escriba la misma lista sin copiarla. */
  readonly marcadas = model<string[]>([]);

  /** Mes que se muestra al abrir. */
  readonly mesInicial = input<Date>(new Date());

  readonly dias = DIAS;
  readonly hoy = isoLocal(new Date());

  private readonly cursor = signal(new Date());

  readonly titulo = computed(() => {
    const c = this.cursor();
    return `${MESES[c.getMonth()]} ${c.getFullYear()}`;
  });

  readonly casillas = computed<Casilla[]>(() => {
    const c = this.cursor();
    const primero = new Date(c.getFullYear(), c.getMonth(), 1);
    const total = new Date(c.getFullYear(), c.getMonth() + 1, 0).getDate();

    // La semana empieza el lunes, igual que en el motor. Empezarla en domingo
    // aquí y en lunes allá haría que «el primer lunes del mes» se viera en una
    // columna y se calculara con otra.
    const desplazamiento = (primero.getDay() + 6) % 7;

    const salida: Casilla[] = [];
    for (let i = 0; i < desplazamiento; i++) {
      salida.push({ fecha: null, iso: `hueco-${i}` });
    }
    for (let d = 1; d <= total; d++) {
      const fecha = new Date(c.getFullYear(), c.getMonth(), d);
      salida.push({ fecha, iso: isoLocal(fecha) });
    }
    return salida;
  });

  constructor() {
    this.cursor.set(new Date(this.mesInicial()));
  }

  mover(meses: number) {
    const c = this.cursor();
    this.cursor.set(new Date(c.getFullYear(), c.getMonth() + meses, 1));
  }

  alternar(iso: string) {
    const actuales = this.marcadas();
    this.marcadas.set(
      actuales.includes(iso)
        ? actuales.filter((x) => x !== iso)
        // Ordenadas: la lista se envía al motor y se enseña al usuario, y unas
        // fechas en el orden en que se pulsaron no se pueden revisar de un vistazo.
        : [...actuales, iso].sort(),
    );
  }
}
