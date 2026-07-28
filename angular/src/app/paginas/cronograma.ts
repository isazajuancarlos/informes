/**
 * Definir una obligación viendo las fechas que produce, antes de guardarla.
 *
 * # Por qué esta pantalla es la más importante del sistema
 *
 * De la regla salen el calendario, las tareas, las alertas y la trazabilidad. Y
 * hasta ahora se definía **a ciegas**: se elegía periodicidad, día y ajuste, se
 * guardaba, y las fechas resultantes solo se veían cuando el planificador ya
 * había creado las tareas. Una regla mal entendida no falla ruidosamente: genera
 * vencimientos distintos de los que la persona creía, o ninguno.
 *
 * # La vista previa usa el MOTOR REAL
 *
 * Llama a `/api/periodicidad/previsualizar`, que ejecuta el mismo motor que
 * genera las tareas en producción. Una vista previa con su propia copia de la
 * lógica sería peor que no tenerla: mostraría fechas que el sistema luego no
 * genera, y nadie sabría cuál de las dos está mal.
 *
 * # Las dos fechas, otra vez
 *
 * Cada vencimiento se muestra con su día nominal y su día límite, y el
 * calendario los pinta distinto. Es la pregunta que de verdad se hace quien crea
 * la obligación: «¿qué día vence?» y «¿hasta cuándo tengo?».
 */
import { DatePipe } from '@angular/common';
import { Component, computed, inject, signal } from '@angular/core';
import { FormsModule } from '@angular/forms';
import { MatButtonModule } from '@angular/material/button';
import { MatButtonToggleModule } from '@angular/material/button-toggle';
import { MatCardModule } from '@angular/material/card';
import { MatCheckboxModule } from '@angular/material/checkbox';
import { MatFormFieldModule } from '@angular/material/form-field';
import { MatIconModule } from '@angular/material/icon';
import { MatInputModule } from '@angular/material/input';
import { MatProgressBarModule } from '@angular/material/progress-bar';
import { MatSelectModule } from '@angular/material/select';
import { MatTooltipModule } from '@angular/material/tooltip';
import { firstValueFrom } from 'rxjs';

import { Api, FilaPrevia, ReglaJson } from '../nucleo/api';

/** El vocabulario del usuario, no el del motor. Nadie define una obligación
 *  pensando en «ESPECIFICO con meses [3,6,9,12]»: piensa «trimestral». */
type Periodicidad = 'mensual' | 'trimestral' | 'semestral' | 'anual' | 'semanal';

const MESES_DE: Record<Periodicidad, number[]> = {
  mensual: [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
  trimestral: [3, 6, 9, 12],
  semestral: [6, 12],
  anual: [12],
  semanal: [],
};

@Component({
  selector: 'siger-cronograma',
  imports: [
    DatePipe,
    FormsModule,
    MatButtonModule,
    MatButtonToggleModule,
    MatCardModule,
    MatCheckboxModule,
    MatFormFieldModule,
    MatIconModule,
    MatInputModule,
    MatProgressBarModule,
    MatSelectModule,
    MatTooltipModule,
  ],
  template: `
    <h2>Cronograma de una obligación</h2>
    <p class="guia">
      Defina la regla y compruebe las fechas <strong>antes</strong> de guardarla. Las
      calcula el mismo motor que después genera las tareas.
    </p>

    <div class="rejilla">
      <mat-card class="regla">
        <mat-card-content>
          <mat-form-field appearance="outline">
            <mat-label>Cada cuánto</mat-label>
            <mat-select [(ngModel)]="periodicidad" (ngModelChange)="cambio()">
              <mat-option value="mensual">Mensual</mat-option>
              <mat-option value="trimestral">Trimestral</mat-option>
              <mat-option value="semestral">Semestral</mat-option>
              <mat-option value="anual">Anual</mat-option>
              <mat-option value="semanal">Semanal</mat-option>
            </mat-select>
          </mat-form-field>

          <!-- Cada periodicidad pide SOLO sus campos. Preguntar de más no es
               inocuo: un formulario que muestra «día de corte» junto a «último
               día del mes» invita a rellenar los dos, y entonces el motor
               descarta uno en silencio. -->
          @if (periodicidad() === 'semanal') {
            <div class="dias">
              @for (d of diasSemana; track d.n) {
                <button mat-stroked-button type="button"
                        [class.activo]="marcados().includes(d.n)"
                        (click)="alternarDia(d.n)">{{ d.etiqueta }}</button>
              }
            </div>
          } @else {
            <mat-button-toggle-group [(ngModel)]="formaDia" (ngModelChange)="cambio()">
              <mat-button-toggle value="corte">Día del mes</mat-button-toggle>
              <mat-button-toggle value="habil"
                matTooltip="«Dentro de los primeros N días hábiles» — no es el día N corrido al hábil siguiente">
                Día hábil
              </mat-button-toggle>
              <mat-button-toggle value="ultimo">Último día</mat-button-toggle>
            </mat-button-toggle-group>

            @if (formaDia() !== 'ultimo') {
              <mat-form-field appearance="outline">
                <mat-label>{{ formaDia() === 'habil' ? 'Qué día hábil' : 'Qué día' }}</mat-label>
                <input matInput type="number" min="1" max="31"
                       [(ngModel)]="dia" (ngModelChange)="cambio()" />
              </mat-form-field>
            }
          }

          <mat-form-field appearance="outline">
            <mat-label>Si cae en día no hábil</mat-label>
            <mat-select [(ngModel)]="ajuste" (ngModelChange)="cambio()">
              <mat-option value="ninguno">No se mueve</mat-option>
              <mat-option value="gracia">Hay plazo hasta el hábil siguiente</mat-option>
              <mat-option value="anticipado">Se entrega el hábil anterior</mat-option>
              <mat-option value="omitir">Ese día no hay obligación</mat-option>
            </mat-select>
          </mat-form-field>

          <mat-form-field appearance="outline">
            <mat-label>Rige desde</mat-label>
            <input matInput type="date" [(ngModel)]="desde" (ngModelChange)="cambio()" />
          </mat-form-field>

          <button mat-flat-button (click)="calcular()" [disabled]="ocupado()">
            <mat-icon>calculate</mat-icon> Ver las fechas
          </button>
        </mat-card-content>
      </mat-card>

      <mat-card class="resultado">
        <mat-card-content>
          @if (ocupado()) { <mat-progress-bar mode="indeterminate" /> }

          @if (error()) {
            <p class="error"><mat-icon>error_outline</mat-icon> {{ error() }}</p>
          }

          @if (!ocupado() && filas().length === 0 && intentado()) {
            <!-- «No genera nada» es un resultado, y hay que decirlo: es el caso
                 en el que la obligación quedaría creada sin hacer nada. -->
            <p class="error">
              <mat-icon>warning</mat-icon>
              Esta regla no genera ningún vencimiento en el año siguiente. Así
              guardada, la obligación existiría y nadie recibiría nunca una tarea.
            </p>
          }

          @if (filas().length > 0) {
            <p class="cuenta">
              <strong>{{ filas().length }}</strong> vencimientos en 12 meses ·
              {{ movidas() }} con el plazo ajustado
            </p>
            <ol class="fechas">
              @for (f of filas(); track f.nominal) {
                <li>
                  <span class="nominal">{{ f.nominal | date: 'EEE dd MMM yyyy' }}</span>
                  @if (f.se_movio) {
                    <mat-icon class="flecha">arrow_right_alt</mat-icon>
                    <span class="limite" [matTooltip]="'Ajuste: ' + f.ajuste">
                      {{ f.limite | date: 'EEE dd MMM' }}
                    </span>
                  }
                </li>
              }
            </ol>
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
    .dias { display: flex; flex-wrap: wrap; gap: .35rem; }
    .dias .activo {
      background: var(--mat-sys-primary); color: var(--mat-sys-on-primary);
    }
    .fechas { margin: 0; padding-left: 1.25rem; columns: 2; }
    @media (max-width: 700px) { .fechas { columns: 1; } }
    .fechas li { margin: .15rem 0; break-inside: avoid; }
    .limite { color: var(--mat-sys-tertiary); }
    .flecha { font-size: 1rem; height: 1rem; width: 1rem; vertical-align: middle; }
    .error { display: flex; align-items: center; gap: .4rem; color: var(--mat-sys-error); }
    .cuenta { color: var(--mat-sys-on-surface-variant); }
  `,
})
export class Cronograma {
  private api = inject(Api);

  readonly diasSemana = [
    { n: 0, etiqueta: 'Lun' }, { n: 1, etiqueta: 'Mar' }, { n: 2, etiqueta: 'Mié' },
    { n: 3, etiqueta: 'Jue' }, { n: 4, etiqueta: 'Vie' }, { n: 5, etiqueta: 'Sáb' },
    { n: 6, etiqueta: 'Dom' },
  ];

  periodicidad = signal<Periodicidad>('mensual');
  formaDia = signal<'corte' | 'habil' | 'ultimo'>('corte');
  dia = signal(15);
  ajuste = signal<'ninguno' | 'gracia' | 'anticipado' | 'omitir'>('gracia');
  desde = signal(new Date().toISOString().slice(0, 10));
  marcados = signal<number[]>([2]);

  filas = signal<FilaPrevia[]>([]);
  ocupado = signal(false);
  intentado = signal(false);
  error = signal('');

  readonly movidas = computed(() => this.filas().filter((f) => f.se_movio).length);

  alternarDia(n: number) {
    const actuales = this.marcados();
    this.marcados.set(actuales.includes(n) ? actuales.filter((x) => x !== n) : [...actuales, n]);
    this.cambio();
  }

  /** Cualquier cambio invalida lo mostrado: dejar en pantalla las fechas de la
   *  regla anterior haría creer que corresponden a la de ahora. */
  cambio() {
    this.filas.set([]);
    this.intentado.set(false);
  }

  private construir(): ReglaJson {
    const base: ReglaJson = {
      modo: this.periodicidad() === 'semanal' ? 'SEMANAL' : 'ESPECIFICO',
      fecha_inicio: this.desde(),
      mover_habil: this.ajuste() === 'gracia',
      mover_habil_anterior: this.ajuste() === 'anticipado',
      omitir_festivos: this.ajuste() === 'omitir',
    };
    if (this.periodicidad() === 'semanal') {
      return { ...base, dias_semana: this.marcados() };
    }
    const meses = MESES_DE[this.periodicidad()];
    switch (this.formaDia()) {
      case 'ultimo':
        return { ...base, meses, es_ultimo_dia: true };
      case 'habil':
        // El n-ésimo hábil ya es hábil: el ajuste no tiene nada que hacer y
        // arrastrarlo movería la fecha una segunda vez.
        return { ...base, meses, dia_habil: this.dia(),
                 mover_habil: false, mover_habil_anterior: false };
      default:
        return { ...base, meses, dia_corte: this.dia() };
    }
  }

  async calcular() {
    this.ocupado.set(true);
    this.error.set('');
    try {
      const desde = this.desde();
      const hasta = new Date(new Date(desde).getTime() + 365 * 86_400_000)
        .toISOString()
        .slice(0, 10);
      this.filas.set(await firstValueFrom(this.api.previsualizar(this.construir(), desde, hasta)));
    } catch (e) {
      this.error.set((e as Error).message);
      this.filas.set([]);
    } finally {
      this.intentado.set(true);
      this.ocupado.set(false);
    }
  }
}
