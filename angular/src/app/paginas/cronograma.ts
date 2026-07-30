/**
 * Definir una obligación marcando en el calendario los días en que vence.
 *
 * # Por qué esta pantalla es la más importante del sistema
 *
 * De la regla salen el calendario, las tareas, las alertas y la trazabilidad. Y
 * una regla mal entendida no falla ruidosamente: genera vencimientos distintos de
 * los que la persona creía, o ninguno.
 *
 * # Se pregunta al revés que antes, y esa es la idea
 *
 * Antes había que ELEGIR la abstracción —periodicidad, día del mes, forma del
 * día— y comprobar después si producía las fechas correctas. Pero quien crea la
 * obligación **tiene las fechas**: están en la resolución, en el contrato, en la
 * circular. Lo que no tiene es el nombre de la abstracción.
 *
 * Así que se marcan los días y el sistema responde qué puede ser eso: «todas las
 * semanas, lunes y jueves», «el 5º día hábil de cada mes», «en marzo, junio,
 * septiembre y diciembre, el día 15». Cada lectura viene con las fechas que
 * generaría, para verlas antes de decidir.
 *
 * # Ninguna lectura se ofrece si no reproduce lo marcado
 *
 * El motor descarta ejecutando, no razonando: una interpretación que no generase
 * alguna de las fechas señaladas no es una lectura de lo que se señaló. Y la
 * última opción es siempre la lista literal, para que ninguna obligación se quede
 * sin poder registrarse por no encajar en una forma que se nos ocurrió.
 *
 * # El ajuste no se deduce
 *
 * De unas fechas marcadas no se sigue si la entidad da plazo hasta el hábil
 * siguiente o exige entregar el anterior: es una decisión, no un patrón. Por eso
 * es un campo, y viaja CON la selección — así las fechas que la lista muestra son
 * las que se van a guardar. Mostrar unas y guardar otras sería el peor defecto
 * posible en una herramienta de plazos.
 */
import { DatePipe } from '@angular/common';
import { Component, effect, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatCardModule } from '@angular/material/card';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { Router } from '@angular/router';
import { firstValueFrom } from 'rxjs';

import { CalendarioMultiple } from '../comun/calendario-multiple';
import { AjusteSel, Api, Lectura } from '../nucleo/api';

/** Las cuatro maneras de tratar un vencimiento que cae en día no hábil, dichas
 *  como las diría quien crea la obligación. */
type Ajuste = 'ninguno' | 'gracia' | 'anticipado' | 'omitir';

@Component({
  selector: 'siger-cronograma',
  imports: [
    CalendarioMultiple,
    DatePipe,
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSelectModule,
  ],
  template: `
    <h2>Cronograma de una obligación</h2>
    <p class="guia">
      Marque en el calendario los días en que vence. El sistema le dirá
      <strong>qué puede ser</strong> y con qué fechas — las calcula el mismo motor
      que después genera las tareas.
    </p>

    <!-- La identidad del informe. Va arriba y separada de la regla: el nombre y
         la descripción son del informe, no de su periodicidad. -->
    <div class="identidad">
      <mat-form-field appearance="outline" subscriptSizing="dynamic">
        <mat-label>Nombre del informe</mat-label>
        <input matInput [(ngModel)]="nombre" maxlength="200"
               (ngModelChange)="errorGuardar.set('')" />
      </mat-form-field>
      <mat-form-field appearance="outline" subscriptSizing="dynamic">
        <mat-label>Descripción (opcional)</mat-label>
        <input matInput [(ngModel)]="descripcion" />
      </mat-form-field>
    </div>

    <div class="rejilla">
      <mat-card>
        <mat-card-content>
          <siger-calendario-multiple [(marcadas)]="fechasMarcadas" />

          <mat-form-field appearance="outline">
            <mat-label>Si el día cae en no hábil</mat-label>
            <mat-select [(ngModel)]="ajuste">
              <mat-option value="ninguno">Vence ese día igual</mat-option>
              <mat-option value="gracia">Hay plazo hasta el hábil siguiente</mat-option>
              <mat-option value="anticipado">Se entrega el hábil anterior</mat-option>
              <mat-option value="omitir">Ese día no hay obligación</mat-option>
            </mat-select>
          </mat-form-field>
          <p class="nota">
            Esto no se deduce de las fechas: es una decisión de la obligación, y
            cambia las fechas que ve a la derecha.
          </p>
        </mat-card-content>
      </mat-card>

      <mat-card>
        <mat-card-content>
          @if (cargando()) { <mat-progress-bar mode="indeterminate" /> }

          @if (error()) {
            <p class="error"><mat-icon>error_outline</mat-icon> {{ error() }}</p>
          }

          @if (fechasMarcadas().length === 0) {
            <p class="vacio">
              <mat-icon>event_available</mat-icon>
              Marque uno o varios días. Con uno solo ya se puede: le diremos si eso
              puede ser mensual, semanal, el último viernes, o solo esa fecha.
            </p>
          } @else if (lecturas().length > 0) {
            <p class="cuenta">
              <strong>{{ lecturas().length }}</strong>
              {{ lecturas().length === 1 ? 'lectura posible' : 'lecturas posibles' }} de
              {{ fechasMarcadas().length }}
              {{ fechasMarcadas().length === 1 ? 'día marcado' : 'días marcados' }}
            </p>

            <!-- Cada lectura se elige entera: su nombre, lo que dice y la regla que
                 se guarda son la misma cosa. No hay manera de guardar una
                 combinación que no se haya visto. -->
            <div class="lecturas">
              @for (l of lecturas(); track l.dice) {
                <button
                  type="button"
                  class="lectura"
                  [class.elegida]="elegida()?.dice === l.dice"
                  (click)="elegir(l)"
                >
                  <span class="cabeza">
                    <mat-icon>{{
                      elegida()?.dice === l.dice ? 'radio_button_checked' : 'radio_button_unchecked'
                    }}</mat-icon>
                    <strong>{{ l.nombre }}</strong>
                  </span>
                  <span class="dice">{{ l.dice }}</span>
                  @if (l.proximas.length > 0) {
                    <span class="proximas">
                      @for (p of l.proximas; track p) {
                        <span class="fecha">{{ p | date: 'dd MMM yyyy' }}</span>
                      }
                    </span>
                  } @else {
                    <!-- Sin fechas no hay obligación que valga, y se dice aquí —en la
                         opción— y no después de guardar. -->
                    <span class="sinfechas">
                      <mat-icon>warning</mat-icon> no genera ningún vencimiento
                    </span>
                  }
                </button>
              }
            </div>

            <div class="acciones">
              <button mat-flat-button (click)="guardar()" [disabled]="!puedeGuardar()">
                <mat-icon>save</mat-icon> Guardar informe
              </button>
              @if (!elegida()) {
                <span class="pista">Elija una lectura para guardar.</span>
              } @else if (!nombre().trim()) {
                <span class="pista">Falta el nombre del informe.</span>
              }
            </div>

            @if (errorGuardar()) {
              <p class="error"><mat-icon>error_outline</mat-icon> {{ errorGuardar() }}</p>
            }
          }
        </mat-card-content>
      </mat-card>
    </div>
  `,
  styles: `
    .guia { color: var(--mat-sys-on-surface-variant); margin-top: -.5rem; }
    .rejilla { display: grid; grid-template-columns: minmax(18rem, 24rem) 1fr; gap: 1rem; }
    @media (max-width: 900px) { .rejilla { grid-template-columns: 1fr; } }
    mat-card-content { display: flex; flex-direction: column; gap: .75rem; padding-top: 1rem; }
    .identidad { display: flex; gap: .75rem; flex-wrap: wrap; margin: .5rem 0 1rem; }
    .identidad mat-form-field { flex: 1 1 16rem; }
    .nota, .cuenta, .pista { color: var(--mat-sys-on-surface-variant); margin: 0; font-size: .9rem; }
    .vacio { display: flex; align-items: center; gap: .5rem; color: var(--mat-sys-on-surface-variant); }
    .error { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-error); }

    .lecturas { display: flex; flex-direction: column; gap: .5rem; }
    .lectura {
      display: flex; flex-direction: column; gap: .25rem; text-align: left;
      padding: .75rem 1rem; border-radius: .75rem; cursor: pointer;
      background: var(--mat-sys-surface-container-low);
      border: 2px solid transparent; color: inherit; font: inherit;
    }
    .lectura:hover { background: var(--mat-sys-surface-container); }
    .lectura.elegida { border-color: var(--mat-sys-primary); }
    .cabeza { display: flex; align-items: center; gap: .5rem; }
    .dice { color: var(--mat-sys-on-surface-variant); }
    .proximas { display: flex; flex-wrap: wrap; gap: .35rem; margin-top: .25rem; }
    .fecha {
      padding: .1rem .5rem; border-radius: 1rem; font-size: .8rem;
      background: var(--mat-sys-secondary-container);
      color: var(--mat-sys-on-secondary-container);
    }
    .sinfechas {
      display: flex; align-items: center; gap: .3rem;
      color: var(--mat-sys-error); font-size: .85rem;
    }
    .acciones { display: flex; align-items: center; gap: .75rem; flex-wrap: wrap; }
  `,
})
export class Cronograma {
  private api = inject(Api);
  private router = inject(Router);

  fechasMarcadas = signal<string[]>([]);
  ajuste = signal<Ajuste>('gracia');

  nombre = signal('');
  descripcion = signal('');

  lecturas = signal<Lectura[]>([]);
  elegida = signal<Lectura | null>(null);
  cargando = signal(false);
  error = signal('');
  errorGuardar = signal('');
  guardando = signal(false);

  constructor() {
    // Cualquier cambio en lo marcado o en el ajuste rehace las lecturas. Dejar en
    // pantalla las de la selección anterior haría creer que corresponden a la de
    // ahora, y ese es justo el error que esta pantalla existe para evitar.
    effect(() => {
      const fechas = this.fechasMarcadas();
      const aj = this.ajuste();
      void this.pedirLecturas(fechas, aj);
    });
  }

  private async pedirLecturas(fechas: string[], ajuste: Ajuste) {
    this.elegida.set(null);
    this.lecturas.set([]);
    this.error.set('');
    if (fechas.length === 0) {
      return;
    }
    this.cargando.set(true);
    try {
      const r = await firstValueFrom(
        this.api.lecturas([...fechas].sort(), this.aAjusteSel(ajuste)),
      );
      this.lecturas.set(r.lecturas);
      // Si solo hay una lectura posible se elige sola: pedir un clic para
      // confirmar lo único que se puede elegir es fricción sin información.
      if (r.lecturas.length === 1) {
        this.elegida.set(r.lecturas[0]);
      }
    } catch (e) {
      this.error.set((e as Error).message);
    } finally {
      this.cargando.set(false);
    }
  }

  private aAjusteSel(a: Ajuste): AjusteSel {
    return {
      gracia: a === 'gracia',
      anticipado: a === 'anticipado',
      omitir_festivos: a === 'omitir',
    };
  }

  elegir(l: Lectura) {
    this.elegida.set(l);
    this.errorGuardar.set('');
  }

  /** Se puede guardar cuando hay nombre, hay una lectura elegida, y esa lectura
   *  produce al menos un vencimiento. Guardar una obligación que no vence nunca la
   *  deja existiendo sin que nadie reciba una tarea jamás. */
  puedeGuardar(): boolean {
    const l = this.elegida();
    return !this.guardando() && !!this.nombre().trim() && !!l && l.proximas.length > 0;
  }

  /** Guarda la regla de la lectura elegida, tal cual la devolvió el motor. No se
   *  reconstruye aquí: lo que se guarda es, byte a byte, la regla cuyas fechas se
   *  mostraron. */
  async guardar() {
    const l = this.elegida();
    const nombre = this.nombre().trim();
    if (!l || !nombre) {
      return;
    }
    this.guardando.set(true);
    this.errorGuardar.set('');
    try {
      const { id } = await firstValueFrom(
        this.api.crearInforme({
          nombre,
          descripcion: this.descripcion().trim() || null,
          recurrencia: l.regla,
        }),
      );
      // A la lista, donde el informe recién creado ya aparece: guardar y no ver el
      // resultado deja la duda de si de verdad quedó.
      await this.router.navigate(['/informes'], { queryParams: { nuevo: id } });
    } catch (e) {
      this.errorGuardar.set((e as Error).message);
    } finally {
      this.guardando.set(false);
    }
  }
}
